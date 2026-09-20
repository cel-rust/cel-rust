use crate::common::types::CelType;
use crate::common::value::{CowVal, Val};
use crate::magic::{Function, FunctionRegistry, IntoFunction};
use crate::objects::{TryIntoValue, Value};
use crate::parser::Expression;
use crate::{DeclarationError, Env, ExecutionError};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Context is a collection of variables and functions that can be used
/// by the interpreter to resolve expressions.
///
/// The context can be either a parent context, or a child context. A
/// parent context is created by default and contains all of the built-in
/// functions. A child context can be created by calling `.new_inner_scope()`. The
/// child context has it's own variables (which can be added to), but it
/// will also reference the parent context. This allows for variables to
/// be overridden within the child context while still being able to
/// resolve variables in the child's parents. You can have theoretically
/// have an infinite number of child contexts that reference each-other.
///
/// So why is this important? Well some CEL-macros such as the `.map` macro
/// declare intermediate user-specified identifiers that should only be
/// available within the macro, and should not override variables in the
/// parent context. The `.map` macro can create a child context from the parent, add the
/// intermediate identifier to the child context, and then evaluate the
/// map expression.
///
/// Intermediate variable stored in child context
///               ↓
/// [1, 2, 3].map(x, x * 2) == [2, 4, 6]
///                  ↑
/// Only in scope for the duration of the map expression
///
/// # Lifetimes
///
/// `'v` bounds the data the context's values may borrow: the
/// [`VariableResolver`] it references and any [`Val`] bound with
/// [`add_variable_as_val`](Context::add_variable_as_val). Values resolved
/// against the context borrow for at most `'v`. `'p` is the borrow of the
/// parent context for a child scope; a root context does not use it.
pub enum Context<'p, 'v> {
    Root {
        functions: FunctionRegistry,
        variables: BTreeMap<String, Box<dyn Val + 'v>>,
        resolver: Option<&'v dyn VariableResolver>,
        env: Arc<Env>,
    },
    Child {
        parent: &'p Context<'p, 'v>,
        variables: BTreeMap<String, Box<dyn Val + 'v>>,
        resolver: Option<&'v dyn VariableResolver>,
    },
}

impl<'p, 'v> Context<'p, 'v> {
    pub fn add_variable<S, V>(
        &mut self,
        name: S,
        value: V,
    ) -> Result<(), <V as TryIntoValue>::Error>
    where
        S: Into<String>,
        V: TryIntoValue,
    {
        let value = value.try_into_value()?;
        let value: Box<dyn Val> = value.try_into().unwrap();
        self.add_variable_as_val(name, value);
        Ok(())
    }

    pub fn add_variable_from_value<S, V>(&mut self, name: S, value: V)
    where
        S: Into<String>,
        V: Into<Value>,
    {
        let value = value.into();
        let value: Box<dyn Val> = value.try_into().unwrap();
        self.add_variable_as_val(name, value);
    }

    /// Binds a variable to a custom [`Val`] implementation directly, without
    /// going through the [`Value`] enum.
    ///
    /// [`add_variable`](Self::add_variable) and
    /// [`add_variable_from_value`](Self::add_variable_from_value) convert their
    /// input into a [`Value`], whose compound variants ([`Value::Map`],
    /// [`Value::List`], and `Value::Struct`) hold eagerly-materialized contents.
    /// For a value that should resolve its contents *on access* instead — e.g.
    /// a large or recursive backing object (a protobuf message, a database
    /// row) where member access maps to
    /// [`Indexer::get`](crate::common::traits::Indexer::get) and is computed
    /// lazily — implement [`Val`] and the relevant operator traits (such as
    /// [`Indexer`](crate::common::traits::Indexer),
    /// [`Iterable`](crate::common::traits::Iterable),
    /// [`Sizer`](crate::common::traits::Sizer)) for your type and bind it here.
    /// The built-in implementations in
    /// [`common::types`](crate::common::types) (e.g. `DefaultMap`, `Struct`)
    /// are the reference for what to implement.
    ///
    /// The value may borrow data for `'v`, for example a
    /// [`CelString`](crate::common::types::CelString) built from a `&'v str`.
    ///
    /// ```ignore
    /// // `my_value` implements `Val` + `Indexer`, resolving fields on access.
    /// let mut ctx = Context::default();
    /// ctx.add_variable_as_val("input", Box::new(my_value));
    /// let program = Program::compile("input.field")?;
    /// // `input.field` calls `Indexer::get` on `my_value` only when evaluated.
    /// let result = program.execute(&ctx)?;
    /// ```
    pub fn add_variable_as_val<S>(&mut self, name: S, value: Box<dyn Val + 'v>)
    where
        S: Into<String>,
    {
        match self {
            Context::Root { variables, .. } => {
                variables.insert(name.into(), value);
            }
            Context::Child { variables, .. } => {
                variables.insert(name.into(), value);
            }
        }
    }

    pub fn set_variable_resolver(&mut self, r: &'v dyn VariableResolver) {
        match self {
            Context::Root { resolver, .. } => {
                *resolver = Some(r);
            }
            Context::Child { resolver, .. } => {
                *resolver = Some(r);
            }
        }
    }

    /// Looks a variable up: the resolver first, then this scope's variables,
    /// then the parent scopes. The result borrows from the context where it
    /// can and is bounded by `'v` where the resolver or a bound value
    /// borrows data.
    pub fn get_variable<'b, S>(&'b self, name: S) -> Option<CowVal<'b, 'v>>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();
        match self {
            Context::Child {
                variables,
                parent,
                resolver,
            } => resolver.and_then(|r| r.resolve(name)).or_else(|| {
                variables
                    .get(name)
                    .map(|b| CowVal::Borrowed(b.as_ref()))
                    .or_else(|| parent.get_variable(name))
            }),
            Context::Root {
                variables,
                resolver,
                ..
            } => resolver
                .and_then(|r| r.resolve(name))
                .or_else(|| variables.get(name).map(|v| CowVal::Borrowed(v.as_ref())))
                .or_else(|| CelType::for_ident(name).map(CowVal::owned)),
        }
    }

    pub(crate) fn env(&self) -> &Env {
        match self {
            Context::Root { env, .. } => env.as_ref(),
            Context::Child { parent, .. } => parent.env(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn get_function(&self, name: &str) -> Option<&Function> {
        match self {
            Context::Root { functions, .. } => functions.get(name),
            Context::Child { parent, .. } => parent.get_function(name),
        }
    }

    /// Adds a function, callable by `name` both as `name(..)` and as a method.
    ///
    /// # Errors
    ///
    /// Fails with [`DeclarationError::OverloadConflict`] if `name` is already
    /// declared as an overload in this context's [`Env`], whether a function or a
    /// member function. When a call is resolved, overloads take precedence over
    /// the functions added here: the function would be shadowed for every call an
    /// overload accepts, and only be reached for the others, so which of the two
    /// answers would depend on the types of the arguments. The standard library
    /// declares overloads for `size`, `contains`, `startsWith`, `string`,
    /// `int`, ... Choose another name, or declare it as an overload with
    /// [`Env::add_overload`] instead.
    ///
    /// # Example
    /// ```
    /// use cel::{Context, DeclarationError};
    ///
    /// let mut context = Context::default();
    /// context.add_function("add", |a: i64, b: i64| a + b).unwrap();
    ///
    /// // `size` is declared as an overload by the standard library
    /// assert_eq!(
    ///     context.add_function("size", |a: i64| a),
    ///     Err(DeclarationError::overload_conflict("size")),
    /// );
    /// ```
    pub fn add_function<T: 'static, F>(
        &mut self,
        name: &str,
        value: F,
    ) -> Result<(), DeclarationError>
    where
        F: IntoFunction<T> + 'static + Send + Sync,
    {
        if let Context::Root { functions, env, .. } = self {
            if env.has_overload(name) || env.has_member_overload(name) {
                return Err(DeclarationError::overload_conflict(name));
            }
            functions.add(name, value);
        };
        Ok(())
    }

    pub fn resolve(&self, expr: &Expression) -> Result<Value, ExecutionError> {
        Value::resolve(expr, self)
    }

    pub fn resolve_all(&self, exprs: &[Expression]) -> Result<Value, ExecutionError> {
        Value::resolve_all(exprs, self)
    }

    /// Creates a child scope that borrows this context as its parent. Values
    /// bound in the child keep the parent's `'v`, so a value resolved in the
    /// child scope can outlive the child.
    pub fn new_inner_scope<'b>(&'b self) -> Context<'b, 'v> {
        Context::Child {
            parent: self,
            variables: Default::default(),
            resolver: None,
        }
    }

    /// Constructs a new empty context with no variables or functions.
    ///
    /// If you're looking for a context that has all the standard methods, functions
    /// and macros already added to the context, use [`Context::default`] instead.
    ///
    /// # Example
    /// ```
    /// use cel::Context;
    /// let mut context = Context::empty();
    /// context.add_function("add", |a: i64, b: i64| a + b).unwrap();
    /// ```
    pub fn empty() -> Self {
        Context::Root {
            env: Arc::new(Env::default()),
            variables: Default::default(),
            functions: Default::default(),
            resolver: None,
        }
    }

    pub fn with_env(env: Arc<Env>) -> Self {
        Context::Root {
            env,
            variables: Default::default(),
            functions: Default::default(),
            resolver: None,
        }
    }
}

impl Default for Context<'_, '_> {
    fn default() -> Self {
        Context::Root {
            env: Arc::new(Env::stdlib()),
            variables: Default::default(),
            functions: Default::default(),
            resolver: None,
        }
    }
}

/// VariableResolver implements a custom resolver for variables that is consulted before looking at
/// variables added to the context. This allows dynamic variables, or avoiding HashMap lookup/creation.
///
/// Unlike [`add_variable`](Context::add_variable) and
/// [`add_variable_from_value`](Context::add_variable_from_value), which convert their input once
/// at bind time, `resolve` runs on *every* lookup of the variable - including every reference to
/// it inside a loop or comprehension. It returns a [`CowVal`] directly (see
/// [`add_variable_as_val`](Context::add_variable_as_val)) rather than a [`Value`], so a resolver
/// backed by something already `Val`-shaped (or by a lazy/recursive object best wrapped directly,
/// per `add_variable_as_val`'s doc) never has to round-trip through `Value` on the hot path - and
/// one that already owns persistent `Val` data (e.g. a table of pre-built values) can hand back a
/// `CowVal::Borrowed` into `self` instead of cloning on every lookup. The value may borrow from
/// the resolver itself, e.g. a [`CelString`](crate::common::types::CelString) built from a `&str`
/// the resolver holds, avoiding a copy.
///
/// # Example
/// ```
/// use cel::common::value::CowVal;
///
/// struct ValueContext {
///     request: cel::Value,
///     response: cel::Value,
/// }
///
/// impl cel::context::VariableResolver for ValueContext {
///     fn resolve<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
///         let value = match variable {
///             "request" => self.request.clone(),
///             "response" => self.response.clone(),
///             _ => return None,
///         };
///         value.try_into().ok().map(CowVal::Owned)
///     }
/// }
/// ```
///
/// Borrowing instead of copying:
/// ```
/// use cel::common::types::CelString;
/// use cel::common::value::CowVal;
///
/// struct Names<'a> {
///     name: &'a str,
/// }
///
/// impl cel::context::VariableResolver for Names<'_> {
///     fn resolve<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
///         match variable {
///             // `CelString::from(&str)` borrows: no copy of the bytes
///             "name" => Some(CowVal::owned(CelString::from(self.name))),
///             _ => None,
///         }
///     }
/// }
/// ```
pub trait VariableResolver: Send + Sync {
    fn resolve<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>>;
}

impl<T: VariableResolver> VariableResolver for Box<T> {
    fn resolve<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
        (**self).resolve(variable)
    }
}

impl<T: VariableResolver> VariableResolver for Arc<T> {
    fn resolve<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
        (**self).resolve(variable)
    }
}

impl<T: VariableResolver> VariableResolver for &T {
    fn resolve<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
        (**self).resolve(variable)
    }
}

#[cfg(test)]
mod test {
    use super::{Context, VariableResolver};
    use crate::common::types::CelString;
    use crate::common::value::CowVal;

    // A helper function that requires T to implement some traits
    fn assert_send<T: Send>() {}

    #[test]
    fn test_context_is_send() {
        // This line will only compile if assertion passes
        assert_send::<super::Context>();
    }

    /// A [`VariableResolver`] can hand back a lazy, `Indexer`-backed [`Val`] directly -
    /// something with no [`Value`] representation at all - rather than being forced to
    /// produce one. Field access on it should still call straight into `Indexer::get`.
    #[test]
    fn test_variable_resolver_returns_val_directly() {
        use crate::common::traits::Indexer;
        use crate::common::types::{CelInt, Type, DYN_TYPE};
        use crate::common::value::{CowVal, Val};
        use crate::{Context, ExecutionError, Program, Value};

        #[derive(Debug)]
        struct Lazy;

        impl Val for Lazy {
            fn get_type(&self) -> &Type {
                &DYN_TYPE
            }

            fn as_indexer<'b, 'v>(&'b self) -> Option<&'b (dyn Indexer + 'v)>
            where
                Self: 'v,
            {
                Some(self)
            }

            fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v> {
                Box::new(Lazy)
            }
        }

        impl Indexer for Lazy {
            fn get<'b, 'v>(&'b self, _idx: &dyn Val) -> Result<CowVal<'b, 'v>, ExecutionError>
            where
                Self: 'v,
            {
                Ok(CowVal::owned(CelInt::from(42)))
            }

            fn steal<'v>(
                self: Box<Self>,
                _idx: &dyn Val,
            ) -> Result<Box<dyn Val + 'v>, ExecutionError>
            where
                Self: 'v,
            {
                Ok(Box::new(CelInt::from(42)))
            }
        }

        struct LazyResolver;

        impl super::VariableResolver for LazyResolver {
            fn resolve<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
                (variable == "input").then(|| CowVal::owned(Lazy))
            }
        }

        let mut ctx = Context::default();
        ctx.set_variable_resolver(&LazyResolver);
        let program = Program::compile("input.anything").unwrap();
        assert_eq!(program.execute(&ctx), Ok(Value::Int(42)));
    }

    /// A [`VariableResolver`] that already owns persistent `Val` data (e.g. a table of
    /// pre-built values) can hand back a `CowVal::Borrowed` into itself instead of cloning
    /// on every lookup - including every reference to the same variable within one
    /// expression.
    #[test]
    fn test_variable_resolver_can_borrow() {
        use crate::common::types::{Type, DYN_TYPE};
        use crate::common::value::{CowVal, Val};
        use crate::{Context, Program};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        #[derive(Debug)]
        struct CountedVal(Arc<AtomicUsize>);

        impl Val for CountedVal {
            fn get_type(&self) -> &Type {
                &DYN_TYPE
            }

            fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::new(CountedVal(self.0.clone()))
            }
        }

        struct BorrowResolver(Box<dyn Val>);

        impl super::VariableResolver for BorrowResolver {
            fn resolve<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
                (variable == "counted").then(|| CowVal::Borrowed(self.0.as_ref()))
            }
        }

        let clones = Arc::new(AtomicUsize::new(0));
        let resolver = BorrowResolver(Box::new(CountedVal(clones.clone())));
        let mut ctx = Context::default();
        ctx.set_variable_resolver(&resolver);

        // Referenced twice - a per-lookup clone would show up as 2, not 0.
        let program = Program::compile("counted == counted").unwrap();
        program.execute(&ctx).unwrap();

        assert_eq!(clones.load(Ordering::SeqCst), 0);
    }

    struct Borrowing<'a>(&'a str);

    impl VariableResolver for Borrowing<'_> {
        fn resolve<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
            (variable == "s").then(|| CowVal::owned(CelString::from(self.0)))
        }
    }

    /// A borrowed string survives a whole evaluation without being copied,
    /// through a conditional, a `string()` call, an optional, a list index,
    /// a map field, and a comprehension.
    #[test]
    fn resolver_value_borrows_through_a_full_evaluation() {
        use crate::parser::Parser;
        use crate::Value;

        let owned = String::from("cel-rust");
        let resolver = Borrowing(owned.as_str());
        let mut ctx = Context::default();
        ctx.set_variable_resolver(&resolver);
        for expr in [
            "s",
            "s == 'cel-rust' ? s : 'other'",
            "string(s)",
            "dyn(s)",
            "optional.of(s).value()",
            "optional.of(s).orValue('other')",
            "[s][0]",
            "{'k': s}.k",
            "{'k': s}['k']",
            "[s].map(x, x)[0]",
            "[1].map(x, s)[0]",
        ] {
            let ast = Parser::default()
                .enable_optional_syntax(true)
                .parse(expr)
                .unwrap();
            let v = Value::resolve_val(&ast, &ctx).unwrap();
            let s = v.downcast_ref::<CelString>().unwrap();
            assert!(
                std::ptr::eq(s.inner(), owned.as_str()),
                "`{expr}` copied the string"
            );
        }
    }

    #[test]
    fn resolver_value_borrows_through_the_context_and_a_child_scope() {
        let owned = String::from("cel-rust");
        let resolver = Borrowing(owned.as_str());
        let mut ctx = Context::default();
        ctx.set_variable_resolver(&resolver);
        let inner = ctx.new_inner_scope();
        let v = inner.get_variable("s").unwrap();
        let s = v.downcast_ref::<CelString>().unwrap();
        assert!(std::ptr::eq(s.inner(), owned.as_str()));
        // the value outlives the child scope: it is bounded by `'v`, not by
        // the scope borrow
        let escaped: Box<dyn crate::common::value::Val + '_> = v.into_owned();
        drop(inner);
        let s = escaped.downcast_ref::<CelString>().unwrap();
        assert!(std::ptr::eq(s.inner(), owned.as_str()));
    }

    /// Overloads take precedence over the functions added to a `Context`, so a
    /// function may not reuse the name of one: a global overload, a member
    /// overload, both, or a namespaced one.
    #[test]
    fn add_function_rejects_a_name_declared_as_an_overload() {
        use crate::{Context, DeclarationError};

        let mut context = Context::default();
        for name in ["int", "startsWith", "size", "optional.of"] {
            assert_eq!(
                context.add_function(name, |a: i64| a),
                Err(DeclarationError::overload_conflict(name)),
                "{name}"
            );
        }
    }

    #[test]
    fn add_function_accepts_a_name_no_overload_uses() {
        use crate::{Context, Program};

        let mut context = Context::default();
        assert_eq!(context.add_function("add", |a: i64, b: i64| a + b), Ok(()));
        let program = Program::compile("add(2, 3)").unwrap();
        assert_eq!(program.execute(&context), Ok(5.into()));
    }

    /// A rejected function must not be registered anyway: were it, `size(1)`
    /// (which no `size` overload accepts) would fall through to it.
    #[test]
    fn a_rejected_function_is_not_registered() {
        use crate::{Context, ExecutionError, Program};

        let mut context = Context::default();
        assert!(context.add_function("size", |_: i64| 42_i64).is_err());

        let program = Program::compile("size(1)").unwrap();
        assert!(matches!(
            program.execute(&context),
            Err(ExecutionError::NoSuchOverload(_))
        ));
        let program = Program::compile("size('abc')").unwrap();
        assert_eq!(program.execute(&context), Ok(3.into()));
    }

    /// The conflict is with the overloads of the context's own `Env`: an empty
    /// one declares none, so the standard names are free.
    #[test]
    fn add_function_only_conflicts_with_the_overloads_of_its_env() {
        use crate::common::types::INT_TYPE;
        use crate::common::value::CowVal;
        use crate::{Context, DeclarationError, Env, ExecutionError};
        use std::sync::Arc;

        assert_eq!(Context::empty().add_function("size", |a: i64| a), Ok(()),);

        fn custom<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
            Ok(args.into_iter().next().unwrap())
        }
        let mut env = Env::default();
        env.add_overload("custom", "custom_int", vec![INT_TYPE], custom)
            .unwrap();
        let mut context = Context::with_env(Arc::new(env));
        assert_eq!(
            context.add_function("custom", |a: i64| a),
            Err(DeclarationError::overload_conflict("custom")),
        );
        assert_eq!(context.add_function("other", |a: i64| a), Ok(()));
    }

    #[test]
    fn add_function_on_a_child_is_a_silent_noop_ignores_dups() {
        use crate::Context;

        let context = Context::default();
        let mut child = context.new_inner_scope();
        assert_eq!(child.add_function("size", |a: i64| a), Ok(()),);
    }
}

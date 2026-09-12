use crate::common::types::CelType;
use crate::common::value::{CowVal, Val};
use crate::magic::{Function, FunctionRegistry, IntoFunction};
use crate::objects::{TryIntoValue, Value};
use crate::parser::Expression;
use crate::{Env, ExecutionError};
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
            } => resolver.and_then(|r| r.resolve_val(name)).or_else(|| {
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
                .and_then(|r| r.resolve_val(name))
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

    pub fn add_function<T: 'static, F>(&mut self, name: &str, value: F)
    where
        F: IntoFunction<T> + 'static + Send + Sync,
    {
        if let Context::Root { functions, .. } = self {
            functions.add(name, value);
        };
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
    /// context.add_function("add", |a: i64, b: i64| a + b);
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
/// Implement [`resolve`](VariableResolver::resolve) to hand out owned
/// [`Value`]s, or [`resolve_val`](VariableResolver::resolve_val) to hand out
/// a [`Val`] that may borrow from the resolver, avoiding a copy.
///
/// # Example
/// ```
/// struct ValueContext {
///     request: cel::Value,
///     response: cel::Value,
/// }
///
/// impl cel::context::VariableResolver for ValueContext {
///     fn resolve(&self, variable: &str) -> Option<cel::Value> {
///         match variable {
///             "request" => Some(self.request.clone()),
///             "response" => Some(self.response.clone()),
///             _ => None,
///         }
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
///     fn resolve_val<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
///         match variable {
///             // `CelString::from(&str)` borrows: no copy of the bytes
///             "name" => Some(CowVal::owned(CelString::from(self.name))),
///             _ => None,
///         }
///     }
/// }
/// ```
pub trait VariableResolver: Send + Sync {
    /// Resolves `variable` to an owned [`Value`].
    ///
    /// The default resolves nothing; implement this or
    /// [`resolve_val`](VariableResolver::resolve_val).
    fn resolve(&self, _variable: &str) -> Option<Value> {
        None
    }

    /// Resolves `variable` to a [`Val`] that may borrow from `self`.
    ///
    /// The default converts the result of
    /// [`resolve`](VariableResolver::resolve).
    fn resolve_val<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
        self.resolve(variable).map(|v| {
            CowVal::Owned(
                v.try_into()
                    .expect("a `Value` always converts to a `Box<dyn Val>`"),
            )
        })
    }
}

impl<T: VariableResolver> VariableResolver for Box<T> {
    fn resolve(&self, variable: &str) -> Option<Value> {
        (**self).resolve(variable)
    }

    fn resolve_val<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
        (**self).resolve_val(variable)
    }
}

impl<T: VariableResolver> VariableResolver for Arc<T> {
    fn resolve(&self, variable: &str) -> Option<Value> {
        (**self).resolve(variable)
    }

    fn resolve_val<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
        (**self).resolve_val(variable)
    }
}

impl<T: VariableResolver> VariableResolver for &T {
    fn resolve(&self, variable: &str) -> Option<Value> {
        (**self).resolve(variable)
    }

    fn resolve_val<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
        (**self).resolve_val(variable)
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

    struct Borrowing<'a>(&'a str);

    impl VariableResolver for Borrowing<'_> {
        fn resolve_val<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
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
}

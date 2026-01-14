use crate::common::value::Val;
use crate::magic::{Function, FunctionRegistry, IntoFunction};
use crate::objects::{TryIntoValue, Value};
use crate::parser::Expression;
use crate::{functions, ExecutionError};
use std::borrow::Cow;
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
pub enum Context<'a> {
    Root {
        functions: FunctionRegistry,
        variables: BTreeMap<String, Box<dyn Val>>,
        resolver: Option<&'a dyn VariableResolver>,
    },
    Child {
        parent: &'a Context<'a>,
        variables: BTreeMap<String, Box<dyn Val>>,
        resolver: Option<&'a dyn VariableResolver>,
    },
}

impl<'a> Context<'a> {
    pub fn add_variable<S, V>(
        &mut self,
        name: S,
        value: V,
    ) -> Result<(), <V as TryIntoValue>::Error>
    where
        S: Into<String>,
        V: TryIntoValue,
    {
        match self {
            Context::Root { variables, .. } => {
                let value = value.try_into_value()?;
                let value: Box<dyn Val> = value.try_into().unwrap();
                variables.insert(name.into(), value);
            }
            Context::Child { variables, .. } => {
                let value = value.try_into_value()?;
                let value: Box<dyn Val> = value.try_into().unwrap();
                variables.insert(name.into(), value);
            }
        }
        Ok(())
    }

    pub fn add_variable_from_value<S, V>(&mut self, name: S, value: V)
    where
        S: Into<String>,
        V: Into<Value>,
    {
        match self {
            Context::Root { variables, .. } => {
                let value = value.into();
                let value: Box<dyn Val> = value.try_into().unwrap();
                variables.insert(name.into(), value);
            }
            Context::Child { variables, .. } => {
                let value = value.into();
                let value: Box<dyn Val> = value.try_into().unwrap();
                variables.insert(name.into(), value);
            }
        }
    }

    pub(crate) fn add_variable_as_val<S>(&mut self, name: S, value: Box<dyn Val>)
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

    pub fn set_variable_resolver(&mut self, r: &'a dyn VariableResolver) {
        match self {
            Context::Root { resolver, .. } => {
                *resolver = Some(r);
            }
            Context::Child { resolver, .. } => {
                *resolver = Some(r);
            }
        }
    }

    pub fn get_variable<S>(&'a self, name: S) -> Option<Cow<'a, dyn Val>>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();
        match self {
            Context::Child {
                variables,
                parent,
                resolver,
            } => resolver
                .and_then(|r| {
                    r.resolve(name)
                        .map(|v| Cow::<dyn Val>::Owned(v.try_into().unwrap()))
                })
                .or_else(|| {
                    variables
                        .get(name)
                        .map(|b| Cow::<dyn Val>::Borrowed(b.as_ref()))
                        .or_else(|| parent.get_variable(name))
                }),
            Context::Root {
                variables,
                resolver,
                ..
            } => resolver
                .and_then(|r| {
                    r.resolve(name)
                        .map(|v| Cow::<dyn Val>::Owned(v.try_into().unwrap()))
                })
                .or_else(|| {
                    variables
                        .get(name)
                        .map(|v| Cow::<dyn Val>::Borrowed(v.as_ref()))
                }),
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

    pub fn new_inner_scope(&self) -> Context<'_> {
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
            variables: Default::default(),
            functions: Default::default(),
            resolver: None,
        }
    }
}

impl Default for Context<'_> {
    fn default() -> Self {
        let mut ctx = Context::Root {
            variables: Default::default(),
            functions: Default::default(),
            resolver: None,
        };

        ctx.add_function("contains", functions::contains);
        ctx.add_function("size", functions::size);
        ctx.add_function("max", functions::max);
        ctx.add_function("min", functions::min);
        ctx.add_function("startsWith", functions::starts_with);
        ctx.add_function("endsWith", functions::ends_with);
        ctx.add_function("string", functions::string);
        ctx.add_function("bytes", functions::bytes);
        ctx.add_function("double", functions::double);
        ctx.add_function("int", functions::int);
        ctx.add_function("uint", functions::uint);
        ctx.add_function("optional.none", functions::optional_none);
        ctx.add_function("optional.of", functions::optional_of);
        ctx.add_function(
            "optional.ofNonZeroValue",
            functions::optional_of_non_zero_value,
        );
        ctx.add_function("value", functions::optional_value);
        ctx.add_function("hasValue", functions::optional_has_value);
        ctx.add_function("or", functions::optional_or_optional);
        ctx.add_function("orValue", functions::optional_or_value);

        #[cfg(feature = "regex")]
        ctx.add_function("matches", functions::matches);

        #[cfg(feature = "chrono")]
        {
            ctx.add_function("duration", functions::duration);
            ctx.add_function("timestamp", functions::timestamp);
            ctx.add_function("getFullYear", functions::time::timestamp_year);
            ctx.add_function("getMonth", functions::time::timestamp_month);
            ctx.add_function("getDayOfYear", functions::time::timestamp_year_day);
            ctx.add_function("getDayOfMonth", functions::time::timestamp_month_day);
            ctx.add_function("getDate", functions::time::timestamp_date);
            ctx.add_function("getDayOfWeek", functions::time::timestamp_weekday);
            ctx.add_function("getHours", functions::time::get_hours);
            ctx.add_function("getMinutes", functions::time::get_minutes);
            ctx.add_function("getSeconds", functions::time::get_seconds);
            ctx.add_function("getMilliseconds", functions::time::get_milliseconds);
        }

        ctx
    }
}

/// VariableResolver implements a custom resolver for variables that is consulted before looking at
/// variables added to the context. This allows dynamic variables, or avoiding HashMap lookup/creation.
///
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
pub trait VariableResolver: Send + Sync {
    fn resolve(&self, variable: &str) -> Option<Value>;
}

impl<T: VariableResolver> VariableResolver for Box<T> {
    fn resolve(&self, variable: &str) -> Option<Value> {
        (**self).resolve(variable)
    }
}

impl<T: VariableResolver> VariableResolver for Arc<T> {
    fn resolve(&self, variable: &str) -> Option<Value> {
        (**self).resolve(variable)
    }
}

impl<T: VariableResolver> VariableResolver for &T {
    fn resolve(&self, variable: &str) -> Option<Value> {
        (**self).resolve(variable)
    }
}

#[cfg(test)]
mod test {
    // A helper function that requires T to implement some traits
    fn assert_send<T: Send>() {}

    #[test]
    fn test_context_is_send() {
        // This line will only compile if assertion passes
        assert_send::<super::Context>();
    }
}

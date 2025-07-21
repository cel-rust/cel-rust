use crate::magic::{Function, FunctionRegistry, IntoFunction};
use crate::objects::{TryIntoValue, Value};
use crate::{functions, ExecutionError};
use cel_parser::Expression;
use std::collections::HashMap;

/// Context is a collection of variables and functions that can be used
/// by the interpreter to resolve expressions.
///
/// The context can be either a parent context, or a child context. A
/// parent context is created by default and contains all of the built-in
/// functions. A child context can be created by calling `.clone()`. The
/// child context has it's own variables (which can be added to), but it
/// will also reference the parent context. This allows for variables to
/// be overridden within the child context while still being able to
/// resolve variables in the child's parents. You can have theoretically
/// have an infinite number of child contexts that reference each-other.
///
/// So why is this important? Well some CEL-macros such as the `.map` macro
/// declare intermediate user-specified identifiers that should only be
/// available within the macro, and should not override variables in the
/// parent context. The `.map` macro can clone the parent context, add the
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
        variables: HashMap<String, Value>,
    },
    Child {
        parent: &'a Context<'a>,
        variables: HashMap<String, Value>,
    },
}

impl Context<'_> {
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
                variables.insert(name.into(), value.try_into_value()?);
            }
            Context::Child { variables, .. } => {
                variables.insert(name.into(), value.try_into_value()?);
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
                variables.insert(name.into(), value.into());
            }
            Context::Child { variables, .. } => {
                variables.insert(name.into(), value.into());
            }
        }
    }

    pub fn get_variable<S>(&self, name: S) -> Result<Value, ExecutionError>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();
        match self {
            Context::Child { variables, parent } => variables
                .get(name)
                .cloned()
                .or_else(|| parent.get_variable(name).ok())
                .ok_or_else(|| ExecutionError::UndeclaredReference(name.to_string().into())),
            Context::Root { variables, .. } => variables
                .get(name)
                .cloned()
                .ok_or_else(|| ExecutionError::UndeclaredReference(name.to_string().into())),
        }
    }

    pub(crate) fn has_function(&self, name: &str) -> bool {
        match self {
            Context::Root { functions, .. } => functions.has(name),
            Context::Child { parent, .. } => parent.has_function(name),
        }
    }

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

    pub fn new_inner_scope(&self) -> Context {
        Context::Child {
            parent: self,
            variables: Default::default(),
        }
    }

    /// Constructs a new empty context with no variables or functions.
    ///
    /// If you're looking for a context that has all the standard methods, functions
    /// and macros already added to the context, use [`Context::default`] instead.
    ///
    /// # Example
    /// ```
    /// use cel_interpreter::Context;
    /// let mut context = Context::empty();
    /// context.add_function("add", |a: i64, b: i64| a + b);
    /// ```
    pub fn empty() -> Self {
        Context::Root {
            variables: Default::default(),
            functions: Default::default(),
        }
    }
}

impl Default for Context<'_> {
    fn default() -> Self {
        let mut ctx = Context::Root {
            variables: Default::default(),
            functions: Default::default(),
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
            ctx.add_function("getHours", functions::time::timestamp_hours);
            ctx.add_function("getMinutes", functions::time::timestamp_minutes);
            ctx.add_function("getSeconds", functions::time::timestamp_seconds);
            ctx.add_function("getMilliseconds", functions::time::timestamp_millis);
        }

        ctx
    }
}

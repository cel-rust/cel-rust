use crate::magic::{Function, FunctionRegistry, IntoFunction};
use crate::objects::{TryIntoValue, Value};
use crate::parser::Expression;
use crate::{functions, ExecutionError};
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
        variables: BTreeMap<String, Value>,
        resolver: Option<&'a dyn VariableResolver>,
        container: Option<String>,
        legacy_enum_semantics: bool,
    },
    Child {
        parent: &'a Context<'a>,
        variables: BTreeMap<String, Value>,
        resolver: Option<&'a dyn VariableResolver>,
        container: Option<String>,
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

    pub fn with_container(mut self, container: String) -> Self {
        match &mut self {
            Context::Root { container: c, .. } => {
                *c = Some(container);
            }
            Context::Child { container: c, .. } => {
                *c = Some(container);
            }
        }
        self
    }

    /// Enable legacy enum semantics where enums are represented as plain integers.
    /// This is needed for compatibility with older CEL implementations.
    pub fn with_legacy_enum_semantics(mut self, legacy: bool) -> Self {
        match &mut self {
            Context::Root {
                legacy_enum_semantics,
                ..
            } => {
                *legacy_enum_semantics = legacy;
            }
            Context::Child { .. } => {
                // Child contexts inherit from parent, cannot set directly
            }
        }
        self
    }

    /// Check if legacy enum semantics are enabled.
    pub fn is_legacy_enum_mode(&self) -> bool {
        match self {
            Context::Root {
                legacy_enum_semantics,
                ..
            } => *legacy_enum_semantics,
            Context::Child { parent, .. } => parent.is_legacy_enum_mode(),
        }
    }

    /// Get the container prefix for type name qualification
    pub fn get_container(&self) -> Option<&str> {
        match self {
            Context::Root { container, .. } => container.as_deref(),
            Context::Child {
                container, parent, ..
            } => {
                // Child context can have its own container, or inherit from parent
                container.as_deref().or_else(|| parent.get_container())
            }
        }
    }

    pub fn get_variable<S>(&self, name: S) -> Result<Value, ExecutionError>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();
        match self {
            Context::Child {
                variables,
                parent,
                resolver,
                container,
            } => {
                // Try container-qualified name first
                if let Some(ref cont) = container {
                    let qualified = format!("{}.{}", cont, name);
                    if let Some(val) = resolver
                        .and_then(|r| r.resolve(&qualified))
                        .or_else(|| variables.get(&qualified).cloned())
                        .or_else(|| parent.get_variable(&qualified).ok())
                    {
                        return Ok(val);
                    }
                }

                // Fall back to unqualified name
                resolver
                    .and_then(|r| r.resolve(name))
                    .or_else(|| {
                        variables
                            .get(name)
                            .cloned()
                            .or_else(|| parent.get_variable(name).ok())
                    })
                    .ok_or_else(|| ExecutionError::UndeclaredReference(name.to_string().into()))
            }
            Context::Root {
                variables,
                resolver,
                container,
                ..
            } => {
                // Try container-qualified name first
                if let Some(ref cont) = container {
                    let qualified = format!("{}.{}", cont, name);
                    if let Some(val) = resolver
                        .and_then(|r| r.resolve(&qualified))
                        .or_else(|| variables.get(&qualified).cloned())
                    {
                        return Ok(val);
                    }
                }

                // Fall back to unqualified name
                resolver
                    .and_then(|r| r.resolve(name))
                    .or_else(|| variables.get(name).cloned())
                    .ok_or_else(|| ExecutionError::UndeclaredReference(name.to_string().into()))
            }
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

    /// Returns the container namespace for type resolution.
    /// When evaluating struct literals, unqualified type names are prefixed with this container.
    pub fn container(&self) -> Option<&str> {
        match self {
            Context::Root { container, .. } => container.as_deref(),
            Context::Child { container, .. } => container.as_deref(),
        }
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
            container: None,
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
            container: None,
            legacy_enum_semantics: false,
        }
    }
}

impl Default for Context<'_> {
    fn default() -> Self {
        let mut ctx = Context::Root {
            variables: Default::default(),
            functions: Default::default(),
            resolver: None,
            container: None,
            legacy_enum_semantics: false,
        };

        ctx.add_function("contains", functions::contains);
        ctx.add_function("size", functions::size);
        ctx.add_function("max", functions::max);
        ctx.add_function("min", functions::min);
        ctx.add_function("startsWith", functions::starts_with);
        ctx.add_function("endsWith", functions::ends_with);
        ctx.add_function("charAt", functions::char_at);
        ctx.add_function("indexOf", functions::index_of);
        ctx.add_function("lastIndexOf", functions::last_index_of);
        ctx.add_function("quote", functions::quote);
        ctx.add_function("strings.quote", functions::quote);
        ctx.add_function("isNaN", functions::is_nan);
        ctx.add_function("math.isNaN", functions::is_nan);
        ctx.add_function("sign", functions::sign);
        ctx.add_function("split", functions::split);
        ctx.add_function("substring", functions::substring);
        ctx.add_function("trim", functions::trim);
        ctx.add_function("replace", functions::replace);
        ctx.add_function("decode", functions::decode);
        ctx.add_function("exists", functions::exists_func);
        ctx.add_function("all", functions::all_func);
        ctx.add_function("existsOne", functions::exists_one_func);
        ctx.add_function("transformList", functions::transform_list);
        ctx.add_function("transformMap", functions::transform_map);
        ctx.add_function("round", functions::round);
        ctx.add_function("join", functions::join);
        ctx.add_function("base64", functions::base64_encode);
        ctx.add_function("base64.encode", functions::base64_encode);
        ctx.add_function("base64.decode", functions::base64_decode);
        ctx.add_function("string", functions::string);
        ctx.add_function("bytes", functions::bytes);
        ctx.add_function("double", functions::double);
        ctx.add_function("int", functions::int);
        ctx.add_function("uint", functions::uint);
        ctx.add_function("dyn", functions::dyn_);
        ctx.add_function("type", functions::type_);
        ctx.add_function("list", functions::list_constructor);
        ctx.add_function("map", functions::map_constructor);
        ctx.add_function("null_type", functions::null_type);
        ctx.add_function("math", functions::math_type);
        ctx.add_function("bool", functions::bool_constructor_default);
        ctx.add_function("bool", functions::bool_constructor);
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
        ctx.add_function("optMap", functions::optional_map);
        ctx.add_function("optFlatMap", functions::optional_flat_map);
        ctx.add_function("greatest", functions::greatest);
        ctx.add_function("least", functions::least);
        ctx.add_function("math.greatest", functions::greatest);
        ctx.add_function("math.least", functions::least);
        ctx.add_function("trunc", functions::trunc);
        ctx.add_function("floor", functions::floor);
        ctx.add_function("ceil", functions::ceil);
        ctx.add_function("isFinite", functions::is_finite);
        ctx.add_function("isInf", functions::is_inf);
        ctx.add_function("bitOr", functions::bit_or);
        ctx.add_function("bitXor", functions::bit_xor);
        ctx.add_function("bitAnd", functions::bit_and);
        ctx.add_function("bitNot", functions::bit_not);
        ctx.add_function("bitShiftLeft", functions::bit_shift_left);
        ctx.add_function("bitShiftRight", functions::bit_shift_right);
        ctx.add_function("abs", functions::abs);
        ctx.add_function("math.abs", functions::abs);
        ctx.add_function("math.sign", functions::sign);
        ctx.add_function("math.pi", functions::math_pi);
        ctx.add_function("math.e", functions::math_e);
        ctx.add_function("math.round", functions::round);
        ctx.add_function("math.floor", functions::floor);
        ctx.add_function("math.ceil", functions::ceil);
        ctx.add_function("math.trunc", functions::trunc);
        ctx.add_function("math.isFinite", functions::is_finite);
        ctx.add_function("math.isInf", functions::is_inf);
        ctx.add_function("math.bitOr", functions::bit_or);
        ctx.add_function("math.bitXor", functions::bit_xor);
        ctx.add_function("math.bitAnd", functions::bit_and);
        ctx.add_function("math.bitNot", functions::bit_not);
        ctx.add_function("math.bitShiftLeft", functions::bit_shift_left);
        ctx.add_function("math.bitShiftRight", functions::bit_shift_right);
        ctx.add_function("lowerAscii", functions::lower_ascii);
        ctx.add_function("upperAscii", functions::upper_ascii);
        ctx.add_function("reverse", functions::reverse);
        ctx.add_function("format", functions::format);

        // Proto extension functions
        ctx.add_function("proto.hasExt", functions::proto_has_ext);
        ctx.add_function("proto.getExt", functions::proto_get_ext);

        ctx.add_variable("type", Value::String(Arc::new("type".to_string())));
        ctx.add_variable(
            "null_type",
            Value::String(Arc::new("null_type".to_string())),
        );
        ctx.add_variable(
            "optional_type",
            Value::String(Arc::new("optional_type".to_string())),
        );
        ctx.add_variable("double", Value::String(Arc::new("double".to_string())));
        ctx.add_variable("bool", Value::String(Arc::new("bool".to_string())));
        ctx.add_variable("int", Value::String(Arc::new("int".to_string())));
        ctx.add_variable("uint", Value::String(Arc::new("uint".to_string())));
        ctx.add_variable("string", Value::String(Arc::new("string".to_string())));
        ctx.add_variable("bytes", Value::String(Arc::new("bytes".to_string())));
        ctx.add_variable("list", Value::String(Arc::new("list".to_string())));
        ctx.add_variable("map", Value::String(Arc::new("map".to_string())));
        ctx.add_variable("math", Value::String(Arc::new("math".to_string())));

        // Google protobuf type constants
        ctx.add_variable(
            "google.protobuf.Timestamp",
            Value::String(Arc::new("google.protobuf.Timestamp".to_string())),
        );
        ctx.add_variable(
            "google.protobuf.Duration",
            Value::String(Arc::new("google.protobuf.Duration".to_string())),
        );

        #[cfg(feature = "regex")]
        ctx.add_function("matches", functions::matches);

        #[cfg(feature = "chrono")]
        {
            ctx.add_function("duration", functions::duration);
            // Overload: duration(duration) -> duration (identity)
            // Register as a separate function that accepts Value
            ctx.add_function("duration", functions::time::duration_value);
            ctx.add_function("timestamp", functions::timestamp);
            // Overload: timestamp(int) -> timestamp (converts Unix timestamp in seconds)
            ctx.add_function("timestamp", functions::time::timestamp_from_int);
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

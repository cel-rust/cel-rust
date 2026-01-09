//! Type environment for CEL type checking.
//!
//! The type environment contains declarations for variables and functions
//! that are available during type checking.

use std::collections::HashMap;
use std::sync::Arc;

use super::types::CelType;

/// A function overload signature.
///
/// Functions in CEL can have multiple overloads with different parameter types.
/// The type checker uses these to resolve which overload applies to a given call.
#[derive(Debug, Clone)]
pub struct FunctionOverload {
    /// Unique identifier for this overload.
    pub overload_id: Arc<str>,

    /// Parameter types (in order).
    /// For instance functions, the first parameter is the receiver type.
    pub param_types: Vec<CelType>,

    /// Return type of this overload.
    pub result_type: CelType,

    /// If true, this is an instance function (method).
    /// The first param_type is the receiver type.
    pub is_instance_function: bool,

    /// Type parameters for generic functions.
    /// Example: ["K", "V"] for a function like `map<K, V>.get(K) -> V`
    pub type_params: Vec<Arc<str>>,
}

impl FunctionOverload {
    /// Create a new function overload.
    pub fn new(
        overload_id: impl Into<Arc<str>>,
        param_types: Vec<CelType>,
        result_type: CelType,
    ) -> Self {
        Self {
            overload_id: overload_id.into(),
            param_types,
            result_type,
            is_instance_function: false,
            type_params: vec![],
        }
    }

    /// Create an instance function (method) overload.
    pub fn instance(
        overload_id: impl Into<Arc<str>>,
        receiver: CelType,
        param_types: Vec<CelType>,
        result_type: CelType,
    ) -> Self {
        let mut all_params = vec![receiver];
        all_params.extend(param_types);
        Self {
            overload_id: overload_id.into(),
            param_types: all_params,
            result_type,
            is_instance_function: true,
            type_params: vec![],
        }
    }

    /// Add type parameters to this overload.
    pub fn with_type_params(mut self, params: Vec<impl Into<Arc<str>>>) -> Self {
        self.type_params = params.into_iter().map(Into::into).collect();
        self
    }

    /// Check if this overload matches the given argument types.
    ///
    /// Returns Some(bindings) if the overload matches, where bindings
    /// contains any inferred type parameter values.
    pub fn matches(
        &self,
        arg_types: &[CelType],
        is_method_call: bool,
    ) -> Option<HashMap<Arc<str>, CelType>> {
        // Check arity
        if is_method_call && self.is_instance_function {
            // Method call: arg_types includes receiver
            if arg_types.len() != self.param_types.len() {
                return None;
            }
        } else if !is_method_call && !self.is_instance_function {
            // Function call: direct match
            if arg_types.len() != self.param_types.len() {
                return None;
            }
        } else if is_method_call && !self.is_instance_function {
            // Method syntax on non-instance function: receiver + args
            if arg_types.len() != self.param_types.len() {
                return None;
            }
        } else {
            // Non-method call on instance function - shouldn't match
            return None;
        }

        // Try to match types, collecting type parameter bindings
        let mut bindings: HashMap<Arc<str>, CelType> = HashMap::new();

        for (arg, param) in arg_types.iter().zip(self.param_types.iter()) {
            if !self.match_type(arg, param, &mut bindings) {
                return None;
            }
        }

        Some(bindings)
    }

    /// Match a single argument type against a parameter type.
    fn match_type(
        &self,
        arg: &CelType,
        param: &CelType,
        bindings: &mut HashMap<Arc<str>, CelType>,
    ) -> bool {
        // Handle type parameters
        if let CelType::TypeParam(name) = param {
            if let Some(bound) = bindings.get(name) {
                // Already bound - check consistency
                return arg.is_assignable_to(bound) || bound.is_assignable_to(arg);
            } else {
                // Bind the type parameter
                bindings.insert(name.clone(), arg.clone());
                return true;
            }
        }

        // Handle dyn
        if param.is_dyn() || arg.is_dyn() {
            return true;
        }

        // Handle error
        if param.is_error() || arg.is_error() {
            return true;
        }

        // Structural matching for parameterized types
        match (arg, param) {
            (CelType::List(arg_elem), CelType::List(param_elem)) => {
                self.match_type(arg_elem, param_elem, bindings)
            }
            (CelType::Map(arg_k, arg_v), CelType::Map(param_k, param_v)) => {
                self.match_type(arg_k, param_k, bindings)
                    && self.match_type(arg_v, param_v, bindings)
            }
            (
                CelType::Abstract(arg_name, arg_params),
                CelType::Abstract(param_name, param_params),
            ) => {
                // Abstract types match if same name and all params match
                if arg_name != param_name || arg_params.len() != param_params.len() {
                    return false;
                }
                for (arg_p, param_p) in arg_params.iter().zip(param_params.iter()) {
                    if !self.match_type(arg_p, param_p, bindings) {
                        return false;
                    }
                }
                true
            }
            _ => arg.is_assignable_to(param),
        }
    }

    /// Get the result type after substituting type parameter bindings.
    pub fn result_type_with_bindings(&self, bindings: &HashMap<Arc<str>, CelType>) -> CelType {
        self.result_type.substitute(bindings)
    }
}

/// Type environment for type checking.
///
/// Contains variable and function declarations, plus container context
/// for qualified name resolution.
#[derive(Debug, Clone, Default)]
pub struct TypeEnv {
    /// Variable declarations: name -> type
    variables: HashMap<Arc<str>, CelType>,

    /// Function declarations: name -> list of overloads
    functions: HashMap<Arc<str>, Vec<FunctionOverload>>,

    /// Container for qualified name resolution.
    /// Example: "google.api.expr.test.v1.proto3"
    container: Option<Arc<str>>,
}

impl TypeEnv {
    /// Create an empty type environment.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a type environment with standard CEL functions.
    pub fn standard() -> Self {
        let mut env = Self::new();
        super::stdlib::register_standard_functions(&mut env);
        env
    }

    /// Set the container for qualified name resolution.
    pub fn set_container(&mut self, container: Option<impl Into<Arc<str>>>) {
        self.container = container.map(Into::into);
    }

    /// Get the container.
    pub fn container(&self) -> Option<&str> {
        self.container.as_deref()
    }

    /// Add a variable declaration.
    pub fn add_variable(&mut self, name: impl Into<Arc<str>>, typ: CelType) {
        self.variables.insert(name.into(), typ);
    }

    /// Add a function overload.
    pub fn add_function(&mut self, name: impl Into<Arc<str>>, overload: FunctionOverload) {
        let name = name.into();
        self.functions.entry(name).or_default().push(overload);
    }

    /// Resolve a variable by name.
    ///
    /// Tries the following in order:
    /// 1. Exact match
    /// 2. Container-qualified match (if container is set)
    pub fn resolve_variable(&self, name: &str) -> Option<&CelType> {
        // Try exact match
        if let Some(typ) = self.variables.get(name) {
            return Some(typ);
        }

        // Try with container prefix
        if let Some(container) = &self.container {
            let qualified = format!("{}.{}", container, name);
            if let Some(typ) = self.variables.get(qualified.as_str()) {
                return Some(typ);
            }
        }

        None
    }

    /// Get all overloads for a function.
    pub fn get_function(&self, name: &str) -> Option<&[FunctionOverload]> {
        self.functions.get(name).map(|v| v.as_slice())
    }

    /// Find matching function overloads for given argument types.
    ///
    /// Returns a list of (overload, type_bindings) pairs for all matching overloads.
    pub fn resolve_function(
        &self,
        name: &str,
        arg_types: &[CelType],
        is_method_call: bool,
    ) -> Vec<(&FunctionOverload, HashMap<Arc<str>, CelType>)> {
        let Some(overloads) = self.functions.get(name) else {
            return vec![];
        };

        overloads
            .iter()
            .filter_map(|ov| ov.matches(arg_types, is_method_call).map(|b| (ov, b)))
            .collect()
    }

    /// Check if a variable exists.
    pub fn has_variable(&self, name: &str) -> bool {
        self.resolve_variable(name).is_some()
    }

    /// Check if a function exists.
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    /// Get all variable names.
    pub fn variable_names(&self) -> impl Iterator<Item = &str> {
        self.variables.keys().map(|s| s.as_ref())
    }

    /// Get all function names.
    pub fn function_names(&self) -> impl Iterator<Item = &str> {
        self.functions.keys().map(|s| s.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variable_lookup() {
        let mut env = TypeEnv::new();
        env.add_variable("x", CelType::Int);
        env.add_variable("y", CelType::String);

        assert_eq!(env.resolve_variable("x"), Some(&CelType::Int));
        assert_eq!(env.resolve_variable("y"), Some(&CelType::String));
        assert_eq!(env.resolve_variable("z"), None);
    }

    #[test]
    fn test_container_qualified_lookup() {
        let mut env = TypeEnv::new();
        env.set_container(Some("com.example"));
        env.add_variable(
            "com.example.Foo",
            CelType::Message("com.example.Foo".into()),
        );

        // Should find via container prefix
        assert!(env.resolve_variable("Foo").is_some());
    }

    #[test]
    fn test_function_overload_matching() {
        let overload =
            FunctionOverload::new("add_int", vec![CelType::Int, CelType::Int], CelType::Int);

        assert!(overload
            .matches(&[CelType::Int, CelType::Int], false)
            .is_some());
        assert!(overload
            .matches(&[CelType::String, CelType::Int], false)
            .is_none());
        assert!(overload.matches(&[CelType::Int], false).is_none());
    }

    #[test]
    fn test_generic_function_matching() {
        let overload = FunctionOverload::new(
            "size_list",
            vec![CelType::List(Box::new(CelType::TypeParam("A".into())))],
            CelType::Int,
        )
        .with_type_params(vec!["A"]);

        let bindings = overload
            .matches(&[CelType::List(Box::new(CelType::String))], false)
            .unwrap();

        assert_eq!(bindings.get("A".into()), Some(&CelType::String));
    }

    #[test]
    fn test_instance_function() {
        let overload = FunctionOverload::instance(
            "contains_string",
            CelType::String,
            vec![CelType::String],
            CelType::Bool,
        );

        // Method call: receiver + args
        assert!(overload
            .matches(&[CelType::String, CelType::String], true)
            .is_some());

        // Wrong receiver type
        assert!(overload
            .matches(&[CelType::Int, CelType::String], true)
            .is_none());
    }
}

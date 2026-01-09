//! CEL type checker for static type analysis.
//!
//! This module provides compile-time type checking for CEL expressions.
//! The type checker validates expressions against a type environment
//! containing variable and function declarations.
//!
//! # Example
//!
//! ```ignore
//! use cel::checker::{TypeChecker, TypeEnv, CelType};
//!
//! let mut env = TypeEnv::standard();
//! env.add_variable("x", CelType::Int);
//!
//! let ast = parse("x + 1")?;
//! let mut checker = TypeChecker::new(&env);
//! let checked = checker.check(&ast)?;
//!
//! // checked.type_map contains the inferred type for each expression
//! ```

pub mod env;
pub mod stdlib;
pub mod types;

pub use env::{FunctionOverload, TypeEnv};
pub use types::{CelType, WrapperType};

use crate::common::ast::{
    BindExpr, CallExpr, ComprehensionExpr, EntryExpr, Expr, IdedExpr, ListExpr, MapExpr,
    SelectExpr, StructExpr,
};
use crate::common::value::CelVal;
use std::collections::HashMap;
use std::sync::Arc;

/// A type checking error.
#[derive(Debug, Clone)]
pub struct TypeError {
    /// The expression ID where the error occurred.
    pub expr_id: u64,
    /// Human-readable error message.
    pub message: String,
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Type error at expr {}: {}", self.expr_id, self.message)
    }
}

impl std::error::Error for TypeError {}

/// A reference to a declared variable or function.
#[derive(Debug, Clone)]
pub enum Reference {
    /// Reference to a variable.
    Variable(Arc<str>),
    /// Reference to a function with possible overload IDs.
    Function(Arc<str>, Vec<Arc<str>>),
}

/// The result of type checking an expression.
#[derive(Debug, Clone)]
pub struct CheckedExpr {
    /// The original expression tree.
    pub expr: IdedExpr,
    /// Map from expression ID to inferred type.
    pub type_map: HashMap<u64, CelType>,
    /// Map from expression ID to reference (for variables/functions).
    pub reference_map: HashMap<u64, Reference>,
}

impl CheckedExpr {
    /// Get the type of the root expression.
    pub fn result_type(&self) -> Option<&CelType> {
        self.type_map.get(&self.expr.id)
    }

    /// Alias for result_type - get the type of the root expression.
    pub fn get_root_type(&self) -> Option<&CelType> {
        self.result_type()
    }
}

/// CEL type checker.
///
/// Performs static type analysis on parsed CEL expressions.
pub struct TypeChecker<'a> {
    env: &'a TypeEnv,
    type_map: HashMap<u64, CelType>,
    reference_map: HashMap<u64, Reference>,
    errors: Vec<TypeError>,
    /// Type parameter bindings during generic resolution.
    #[allow(dead_code)]
    type_bindings: HashMap<Arc<str>, CelType>,
    /// Scoped variable types (for comprehensions).
    scoped_vars: Vec<HashMap<String, CelType>>,
}

impl<'a> TypeChecker<'a> {
    /// Create a new type checker with the given type environment.
    pub fn new(env: &'a TypeEnv) -> Self {
        Self {
            env,
            type_map: HashMap::new(),
            reference_map: HashMap::new(),
            errors: Vec::new(),
            type_bindings: HashMap::new(),
            scoped_vars: Vec::new(),
        }
    }

    /// Check an expression and return the checked result.
    ///
    /// Returns an error if type checking fails with errors.
    pub fn check_expr(&mut self, expr: &IdedExpr) -> Result<CheckedExpr, Vec<TypeError>> {
        self.infer(expr);

        if self.errors.is_empty() {
            Ok(CheckedExpr {
                expr: expr.clone(),
                type_map: std::mem::take(&mut self.type_map),
                reference_map: std::mem::take(&mut self.reference_map),
            })
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    /// Infer the type of an expression.
    pub fn infer(&mut self, expr: &IdedExpr) -> CelType {
        let typ = match &expr.expr {
            Expr::Literal(lit) => self.infer_literal(lit),
            Expr::Ident(name) => self.infer_ident(expr.id, name),
            Expr::Select(sel) => self.infer_select(expr.id, sel),
            Expr::Call(call) => self.infer_call(expr.id, call),
            Expr::List(list) => self.infer_list(list),
            Expr::Map(map) => self.infer_map(map),
            Expr::Struct(s) => self.infer_struct(s),
            Expr::Comprehension(comp) => self.infer_comprehension(expr.id, comp),
            Expr::Bind(bind) => self.infer_bind(bind),
            Expr::Unspecified => CelType::Error,
        };

        self.type_map.insert(expr.id, typ.clone());
        typ
    }

    /// Infer the type of a literal value.
    fn infer_literal(&self, lit: &CelVal) -> CelType {
        match lit {
            CelVal::Boolean(_) => CelType::Bool,
            CelVal::Int(_) => CelType::Int,
            CelVal::UInt(_) => CelType::Uint,
            CelVal::Double(_) => CelType::Double,
            CelVal::String(_) => CelType::String,
            CelVal::Bytes(_) => CelType::Bytes,
            CelVal::Null => CelType::Null,
            CelVal::Duration(_) => CelType::Duration,
            CelVal::Timestamp(_) => CelType::Timestamp,
            _ => CelType::Dyn, // Other types like Error, Dyn, etc.
        }
    }

    /// Infer the type of an identifier.
    fn infer_ident(&mut self, id: u64, name: &str) -> CelType {
        // Check scoped variables first (comprehension variables)
        for scope in self.scoped_vars.iter().rev() {
            if let Some(typ) = scope.get(name) {
                self.reference_map
                    .insert(id, Reference::Variable(name.into()));
                return typ.clone();
            }
        }

        // Check environment
        if let Some(typ) = self.env.resolve_variable(name) {
            self.reference_map
                .insert(id, Reference::Variable(name.into()));
            return typ.clone();
        }

        // Check if it's a type name
        match name {
            "bool" => return CelType::Type(Box::new(CelType::Bool)),
            "int" => return CelType::Type(Box::new(CelType::Int)),
            "uint" => return CelType::Type(Box::new(CelType::Uint)),
            "double" => return CelType::Type(Box::new(CelType::Double)),
            "string" => return CelType::Type(Box::new(CelType::String)),
            "bytes" => return CelType::Type(Box::new(CelType::Bytes)),
            "list" => return CelType::Type(Box::new(CelType::List(Box::new(CelType::Dyn)))),
            "map" => {
                return CelType::Type(Box::new(CelType::Map(
                    Box::new(CelType::Dyn),
                    Box::new(CelType::Dyn),
                )))
            }
            "null_type" => return CelType::Type(Box::new(CelType::Null)),
            "type" => return CelType::Type(Box::new(CelType::Type(Box::new(CelType::Dyn)))),
            _ => {}
        }

        // Check if it's a function name (for function references)
        if self.env.has_function(name) {
            // Return dyn for now - function types are complex
            return CelType::Dyn;
        }

        self.errors.push(TypeError {
            expr_id: id,
            message: format!("undeclared reference to '{}'", name),
        });
        CelType::Error
    }

    /// Infer the type of a select expression (field access).
    fn infer_select(&mut self, id: u64, sel: &SelectExpr) -> CelType {
        let operand_type = self.infer(&sel.operand);

        // Handle error propagation
        if operand_type.is_error() {
            return CelType::Error;
        }

        // Handle dyn
        if operand_type.is_dyn() {
            return CelType::Dyn;
        }

        match &operand_type {
            CelType::Map(_, value_type) => {
                // Map field access returns the value type
                (**value_type).clone()
            }
            CelType::Message(msg_name) => {
                // For proto messages, we'd need a message type provider
                // For now, return dyn
                let _ = msg_name;
                CelType::Dyn
            }
            CelType::Timestamp => {
                // Timestamp fields like getFullYear, etc. are handled as methods
                self.errors.push(TypeError {
                    expr_id: id,
                    message: format!("no field '{}' on timestamp (use method syntax)", sel.field),
                });
                CelType::Error
            }
            CelType::Duration => {
                self.errors.push(TypeError {
                    expr_id: id,
                    message: format!("no field '{}' on duration (use method syntax)", sel.field),
                });
                CelType::Error
            }
            _ => {
                self.errors.push(TypeError {
                    expr_id: id,
                    message: format!(
                        "type '{}' does not support field access '{}'",
                        operand_type, sel.field
                    ),
                });
                CelType::Error
            }
        }
    }

    /// Infer the type of a function call.
    fn infer_call(&mut self, id: u64, call: &CallExpr) -> CelType {
        // Check if this is a qualified function call like "optional.none()"
        // where the target is an identifier and the function name creates a qualified name
        if let Some(target) = &call.target {
            if let Expr::Ident(namespace) = &target.expr {
                // Try to resolve as a qualified function name (e.g., "optional.none")
                let qualified_name = format!("{}.{}", namespace, call.func_name);
                let arg_types: Vec<CelType> = call.args.iter().map(|arg| self.infer(arg)).collect();

                // Check if any arg is an error
                if arg_types.iter().any(|t| t.is_error()) {
                    return CelType::Error;
                }

                let matches = self
                    .env
                    .resolve_function(&qualified_name, &arg_types, false);
                if !matches.is_empty() {
                    // Found a qualified function, use it
                    let (overload, bindings) = &matches[0];
                    self.reference_map.insert(
                        id,
                        Reference::Function(
                            Arc::from(qualified_name.as_str()),
                            vec![overload.overload_id.clone()],
                        ),
                    );
                    return overload.result_type.substitute(bindings);
                }
            }
        }

        // Infer target type if present (method call)
        let target_type = call.target.as_ref().map(|t| self.infer(t));
        let is_method_call = target_type.is_some();

        // Infer argument types
        let arg_types: Vec<CelType> = call.args.iter().map(|arg| self.infer(arg)).collect();

        // Build full argument list (receiver + args for methods)
        let all_arg_types: Vec<CelType> = match &target_type {
            Some(t) => std::iter::once(t.clone()).chain(arg_types).collect(),
            None => arg_types,
        };

        // Check for error propagation
        if all_arg_types.iter().any(|t| t.is_error()) {
            return CelType::Error;
        }

        // Handle special operators
        match call.func_name.as_str() {
            // Conditional operator
            "_?_:_" => {
                if all_arg_types.len() == 3 {
                    // condition ? true_branch : false_branch
                    // Result is common type of branches
                    return all_arg_types[1].common_type(&all_arg_types[2]);
                }
            }
            // Index operator
            "_[_]" => {
                if all_arg_types.len() == 2 {
                    match &all_arg_types[0] {
                        CelType::List(elem) => return (**elem).clone(),
                        CelType::Map(_, val) => return (**val).clone(),
                        CelType::Dyn => return CelType::Dyn,
                        _ => {
                            self.errors.push(TypeError {
                                expr_id: id,
                                message: format!(
                                    "type '{}' does not support indexing",
                                    all_arg_types[0]
                                ),
                            });
                            return CelType::Error;
                        }
                    }
                }
            }
            _ => {}
        }

        // Resolve function overloads
        let matches = self
            .env
            .resolve_function(&call.func_name, &all_arg_types, is_method_call);

        match matches.len() {
            0 => {
                // Check if any argument is dyn - if so, return dyn
                if all_arg_types.iter().any(|t| t.is_dyn()) {
                    return CelType::Dyn;
                }

                self.errors.push(TypeError {
                    expr_id: id,
                    message: format!(
                        "no matching overload for '{}' with argument types ({})",
                        call.func_name,
                        all_arg_types
                            .iter()
                            .map(|t| format!("{}", t))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                });
                CelType::Error
            }
            1 => {
                let (overload, bindings) = &matches[0];
                self.reference_map.insert(
                    id,
                    Reference::Function(
                        call.func_name.clone().into(),
                        vec![overload.overload_id.clone()],
                    ),
                );
                overload.result_type_with_bindings(bindings)
            }
            _ => {
                // Multiple matches - record all overload IDs
                let ids: Vec<Arc<str>> = matches
                    .iter()
                    .map(|(ov, _)| ov.overload_id.clone())
                    .collect();
                self.reference_map
                    .insert(id, Reference::Function(call.func_name.clone().into(), ids));

                // Return the result type (should be same for all matching overloads)
                let (overload, bindings) = &matches[0];
                overload.result_type_with_bindings(bindings)
            }
        }
    }

    /// Infer the type of a list literal.
    fn infer_list(&mut self, list: &ListExpr) -> CelType {
        if list.elements.is_empty() {
            // Empty list has element type dyn
            return CelType::List(Box::new(CelType::Dyn));
        }

        // Infer element types
        let elem_types: Vec<CelType> = list.elements.iter().map(|e| self.infer(e)).collect();

        // Find common element type
        let mut common = elem_types[0].clone();
        for t in &elem_types[1..] {
            common = common.common_type(t);
        }

        CelType::List(Box::new(common))
    }

    /// Infer the type of a map literal.
    fn infer_map(&mut self, map: &MapExpr) -> CelType {
        if map.entries.is_empty() {
            return CelType::Map(Box::new(CelType::Dyn), Box::new(CelType::Dyn));
        }

        let mut key_types = Vec::new();
        let mut value_types = Vec::new();

        for entry in &map.entries {
            if let EntryExpr::MapEntry(map_entry) = &entry.expr {
                key_types.push(self.infer(&map_entry.key));
                value_types.push(self.infer(&map_entry.value));
            }
        }

        if key_types.is_empty() {
            return CelType::Map(Box::new(CelType::Dyn), Box::new(CelType::Dyn));
        }

        // Find common key and value types
        let mut common_key = key_types[0].clone();
        for k in &key_types[1..] {
            common_key = common_key.common_type(k);
        }

        let mut common_value = value_types[0].clone();
        for v in &value_types[1..] {
            common_value = common_value.common_type(v);
        }

        CelType::Map(Box::new(common_key), Box::new(common_value))
    }

    /// Infer the type of a struct literal.
    fn infer_struct(&mut self, s: &StructExpr) -> CelType {
        // Infer field types
        for entry in &s.entries {
            if let EntryExpr::StructField(field) = &entry.expr {
                self.infer(&field.value);
            }
        }

        // Qualify the type name using the container if not already qualified
        let type_name = if s.type_name.contains('.') {
            s.type_name.clone()
        } else if let Some(container) = self.env.container() {
            format!("{}.{}", container, s.type_name)
        } else {
            s.type_name.clone()
        };

        // Return the message type
        CelType::Message(type_name.into())
    }

    /// Infer the type of a comprehension expression.
    fn infer_comprehension(&mut self, id: u64, comp: &ComprehensionExpr) -> CelType {
        // Infer the iteration range type
        let range_type = self.infer(&comp.iter_range);

        // Determine element type
        let elem_type = match &range_type {
            CelType::List(elem) => (**elem).clone(),
            CelType::Map(key, _) => (**key).clone(),
            CelType::Dyn => CelType::Dyn,
            _ => {
                self.errors.push(TypeError {
                    expr_id: id,
                    message: format!("cannot iterate over type '{}'", range_type),
                });
                CelType::Error
            }
        };

        // Push a new scope with iteration variable
        let mut scope = HashMap::new();
        scope.insert(comp.iter_var.clone(), elem_type);

        // Add accumulator variable if present
        if !comp.accu_var.is_empty() {
            let accu_type = self.infer(&comp.accu_init);
            scope.insert(comp.accu_var.clone(), accu_type);
        }

        self.scoped_vars.push(scope);

        // Infer types of loop body
        self.infer(&comp.loop_cond);
        self.infer(&comp.loop_step);
        let result_type = self.infer(&comp.result);

        self.scoped_vars.pop();

        result_type
    }

    /// Infer the type of a bind expression.
    fn infer_bind(&mut self, bind: &BindExpr) -> CelType {
        // Infer the initializer type
        let init_type = self.infer(&bind.init);

        // Push scope with bound variable
        let mut scope = HashMap::new();
        scope.insert(bind.var.clone(), init_type);
        self.scoped_vars.push(scope);

        // Infer result type
        let result_type = self.infer(&bind.result);

        self.scoped_vars.pop();

        result_type
    }

    /// Get all collected errors.
    pub fn errors(&self) -> &[TypeError] {
        &self.errors
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_literal_int(id: u64, val: i64) -> IdedExpr {
        IdedExpr {
            id,
            expr: Expr::Literal(CelVal::Int(val)),
        }
    }

    fn make_literal_string(id: u64, val: &str) -> IdedExpr {
        IdedExpr {
            id,
            expr: Expr::Literal(CelVal::String(val.to_string())),
        }
    }

    fn make_ident(id: u64, name: &str) -> IdedExpr {
        IdedExpr {
            id,
            expr: Expr::Ident(name.to_string()),
        }
    }

    fn make_call(id: u64, name: &str, args: Vec<IdedExpr>) -> IdedExpr {
        IdedExpr {
            id,
            expr: Expr::Call(CallExpr {
                target: None,
                func_name: name.to_string(),
                args,
            }),
        }
    }

    fn make_method_call(id: u64, target: IdedExpr, name: &str, args: Vec<IdedExpr>) -> IdedExpr {
        IdedExpr {
            id,
            expr: Expr::Call(CallExpr {
                target: Some(Box::new(target)),
                func_name: name.to_string(),
                args,
            }),
        }
    }

    #[test]
    fn test_literal_types() {
        let env = TypeEnv::standard();
        let mut checker = TypeChecker::new(&env);

        let int_lit = make_literal_int(1, 42);
        assert_eq!(checker.infer(&int_lit), CelType::Int);

        let str_lit = make_literal_string(2, "hello");
        assert_eq!(checker.infer(&str_lit), CelType::String);
    }

    #[test]
    fn test_variable_lookup() {
        let mut env = TypeEnv::standard();
        env.add_variable("x", CelType::Int);

        let mut checker = TypeChecker::new(&env);

        let ident = make_ident(1, "x");
        assert_eq!(checker.infer(&ident), CelType::Int);
        assert!(checker.reference_map.contains_key(&1));
    }

    #[test]
    fn test_undeclared_variable() {
        let env = TypeEnv::standard();
        let mut checker = TypeChecker::new(&env);

        let ident = make_ident(1, "undefined_var");
        assert_eq!(checker.infer(&ident), CelType::Error);
        assert!(!checker.errors.is_empty());
    }

    #[test]
    fn test_addition() {
        let env = TypeEnv::standard();
        let mut checker = TypeChecker::new(&env);

        // 1 + 2
        let expr = make_call(
            1,
            "_+_",
            vec![make_literal_int(2, 1), make_literal_int(3, 2)],
        );

        assert_eq!(checker.infer(&expr), CelType::Int);
        assert!(checker.errors.is_empty());
    }

    #[test]
    fn test_string_concat() {
        let env = TypeEnv::standard();
        let mut checker = TypeChecker::new(&env);

        // "hello" + " world"
        let expr = make_call(
            1,
            "_+_",
            vec![
                make_literal_string(2, "hello"),
                make_literal_string(3, " world"),
            ],
        );

        assert_eq!(checker.infer(&expr), CelType::String);
    }

    #[test]
    fn test_method_call() {
        let env = TypeEnv::standard();
        let mut checker = TypeChecker::new(&env);

        // "hello".contains("ell")
        let expr = make_method_call(
            1,
            make_literal_string(2, "hello"),
            "contains",
            vec![make_literal_string(3, "ell")],
        );

        assert_eq!(checker.infer(&expr), CelType::Bool);
    }

    #[test]
    fn test_list_literal() {
        let env = TypeEnv::standard();
        let mut checker = TypeChecker::new(&env);

        // [1, 2, 3]
        let expr = IdedExpr {
            id: 1,
            expr: Expr::List(ListExpr {
                elements: vec![
                    make_literal_int(2, 1),
                    make_literal_int(3, 2),
                    make_literal_int(4, 3),
                ],
                optional_indices: vec![],
            }),
        };

        assert_eq!(checker.infer(&expr), CelType::List(Box::new(CelType::Int)));
    }

    #[test]
    fn test_size_function() {
        let env = TypeEnv::standard();
        let mut checker = TypeChecker::new(&env);

        // size("hello")
        let expr = make_call(1, "size", vec![make_literal_string(2, "hello")]);

        assert_eq!(checker.infer(&expr), CelType::Int);
    }
}

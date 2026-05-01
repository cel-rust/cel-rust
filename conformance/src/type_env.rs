//! Type environment stubs for conformance tests.
//!
//! The full type checker is not available on this branch, so we provide minimal stubs.
//! Type checking tests will fail.

use crate::proto::cel::expr::{Decl, Type};

/// Stub type environment - type checking is not supported on this branch
pub struct TypeEnv;

impl Default for TypeEnv {
    fn default() -> Self {
        TypeEnv
    }
}

/// Build a type environment from declarations - returns stub since checker is not available
pub fn build_type_env_from_decls(_decls: &[Decl], _container: &Option<String>) -> TypeEnv {
    TypeEnv
}

/// Convert proto type to string description (for error messages)
pub fn proto_type_to_cel_type(proto_type: &Type) -> String {
    format!("{:?}", proto_type)
}

/// Compare proto types - always returns false since type checking is not available
pub fn types_equal(_expected: &Type, _actual: &str) -> bool {
    false
}

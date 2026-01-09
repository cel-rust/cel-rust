//! CEL type system for static type checking.
//!
//! This module defines the type representation used by the CEL type checker.
//! Types follow the CEL specification and proto definitions in `checked.proto`.

use std::fmt;
use std::sync::Arc;

/// CEL type representation.
///
/// This enum represents all possible types in CEL, including:
/// - Primitive types (bool, int, uint, double, string, bytes)
/// - Collection types (list, map)
/// - Special types (null, dyn, error, type)
/// - Proto message types
/// - Wrapper types (for proto wrappers like Int32Value)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CelType {
    /// Dynamic type - accepts any value at runtime.
    /// Used when type cannot be determined statically.
    Dyn,

    /// Null type - the type of the null value.
    Null,

    /// Boolean type.
    Bool,

    /// Signed 64-bit integer type.
    Int,

    /// Unsigned 64-bit integer type.
    Uint,

    /// 64-bit floating point type.
    Double,

    /// String type (UTF-8).
    String,

    /// Bytes type (arbitrary byte sequence).
    Bytes,

    /// Timestamp type (google.protobuf.Timestamp).
    Timestamp,

    /// Duration type (google.protobuf.Duration).
    Duration,

    /// Any type (google.protobuf.Any).
    Any,

    /// List type with element type.
    /// Example: `list<int>` is `List(Box::new(CelType::Int))`
    List(Box<CelType>),

    /// Map type with key and value types.
    /// Example: `map<string, int>` is `Map(Box::new(CelType::String), Box::new(CelType::Int))`
    Map(Box<CelType>, Box<CelType>),

    /// Proto message type with fully qualified name.
    /// Example: `Message("google.protobuf.Duration".into())`
    Message(Arc<str>),

    /// Type parameter for generic functions.
    /// Example: In `list<A>`, the `A` is `TypeParam("A".into())`
    TypeParam(Arc<str>),

    /// The metatype - the type of a type.
    /// Example: `type(int)` returns a value of type `Type(Box::new(CelType::Int))`
    Type(Box<CelType>),

    /// Wrapper types for proto well-known wrapper types.
    /// These are nullable versions of primitives.
    Wrapper(WrapperType),

    /// Function type with parameter types and return type.
    /// Example: `(int, int) -> bool` is `Function(vec![Int, Int], Box::new(Bool))`
    Function(Vec<CelType>, Box<CelType>),

    /// Abstract type with name and type parameters.
    /// Example: `optional_type<int>` is `Abstract("optional_type".into(), vec![Int])`
    Abstract(Arc<str>, Vec<CelType>),

    /// Error type - used to propagate type errors through expressions.
    /// When a subexpression has a type error, the parent gets Error type.
    Error,
}

/// Wrapper types corresponding to proto well-known wrapper messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WrapperType {
    Bool,
    Int64,
    Int32,
    Uint64,
    Uint32,
    Double,
    Float,
    String,
    Bytes,
}

impl CelType {
    /// Returns true if this type is the dynamic type.
    pub fn is_dyn(&self) -> bool {
        matches!(self, CelType::Dyn)
    }

    /// Returns true if this type is the error type.
    pub fn is_error(&self) -> bool {
        matches!(self, CelType::Error)
    }

    /// Returns true if this type is a primitive type.
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            CelType::Bool
                | CelType::Int
                | CelType::Uint
                | CelType::Double
                | CelType::String
                | CelType::Bytes
        )
    }

    /// Returns true if this type can be used as a map key.
    /// CEL only allows bool, int, uint, and string as map keys.
    pub fn is_map_key(&self) -> bool {
        matches!(
            self,
            CelType::Bool | CelType::Int | CelType::Uint | CelType::String
        )
    }

    /// Check if this type is assignable to the target type.
    ///
    /// Type A is assignable to type B if a value of type A can be used
    /// where a value of type B is expected.
    ///
    /// Key rules:
    /// - Any type is assignable to Dyn
    /// - Dyn is assignable to any type (runtime check)
    /// - Null is assignable to wrapper types and message types
    /// - List/Map are covariant in their type parameters
    /// - Error propagates (Error assignable to anything, anything to Error)
    pub fn is_assignable_to(&self, target: &CelType) -> bool {
        // Error type propagates
        if self.is_error() || target.is_error() {
            return true;
        }

        // Dyn accepts anything and is accepted anywhere
        if self.is_dyn() || target.is_dyn() {
            return true;
        }

        // Same types are always assignable
        if self == target {
            return true;
        }

        match (self, target) {
            // Null is assignable to wrapper types (nullable primitives)
            (CelType::Null, CelType::Wrapper(_)) => true,
            // Null is assignable to message types (proto messages are nullable)
            (CelType::Null, CelType::Message(_)) => true,
            // Null is assignable to Any
            (CelType::Null, CelType::Any) => true,

            // Primitives are assignable to their wrapper types
            (CelType::Bool, CelType::Wrapper(WrapperType::Bool)) => true,
            (CelType::Int, CelType::Wrapper(WrapperType::Int64 | WrapperType::Int32)) => true,
            (CelType::Uint, CelType::Wrapper(WrapperType::Uint64 | WrapperType::Uint32)) => true,
            (CelType::Double, CelType::Wrapper(WrapperType::Double | WrapperType::Float)) => true,
            (CelType::String, CelType::Wrapper(WrapperType::String)) => true,
            (CelType::Bytes, CelType::Wrapper(WrapperType::Bytes)) => true,

            // Wrapper types are assignable to their primitive types
            (CelType::Wrapper(WrapperType::Bool), CelType::Bool) => true,
            (CelType::Wrapper(WrapperType::Int64 | WrapperType::Int32), CelType::Int) => true,
            (CelType::Wrapper(WrapperType::Uint64 | WrapperType::Uint32), CelType::Uint) => true,
            (CelType::Wrapper(WrapperType::Double | WrapperType::Float), CelType::Double) => true,
            (CelType::Wrapper(WrapperType::String), CelType::String) => true,
            (CelType::Wrapper(WrapperType::Bytes), CelType::Bytes) => true,

            // List covariance: list<A> assignable to list<B> if A assignable to B
            (CelType::List(elem_a), CelType::List(elem_b)) => elem_a.is_assignable_to(elem_b),

            // Map covariance in both key and value
            (CelType::Map(key_a, val_a), CelType::Map(key_b, val_b)) => {
                key_a.is_assignable_to(key_b) && val_a.is_assignable_to(val_b)
            }

            // Type parameters match if they have the same name
            (CelType::TypeParam(a), CelType::TypeParam(b)) => a == b,

            // Message types match by name
            (CelType::Message(a), CelType::Message(b)) => a == b,

            // Any accepts any message type
            (CelType::Message(_), CelType::Any) => true,

            // Timestamp and Duration are message types
            (CelType::Timestamp, CelType::Message(name))
                if name.as_ref() == "google.protobuf.Timestamp" =>
            {
                true
            }
            (CelType::Duration, CelType::Message(name))
                if name.as_ref() == "google.protobuf.Duration" =>
            {
                true
            }

            // Abstract types are assignable if same name and params are assignable
            (CelType::Abstract(name_a, params_a), CelType::Abstract(name_b, params_b)) => {
                name_a == name_b
                    && params_a.len() == params_b.len()
                    && params_a
                        .iter()
                        .zip(params_b.iter())
                        .all(|(a, b)| a.is_assignable_to(b))
            }

            _ => false,
        }
    }

    /// Find the common supertype of two types.
    ///
    /// This is used for conditional expressions (a ? b : c) where
    /// we need to find a type that both branches can be assigned to.
    pub fn common_type(&self, other: &CelType) -> CelType {
        // Same types
        if self == other {
            return self.clone();
        }

        // Error propagates
        if self.is_error() {
            return other.clone();
        }
        if other.is_error() {
            return self.clone();
        }

        // Dyn is the universal supertype
        if self.is_dyn() || other.is_dyn() {
            return CelType::Dyn;
        }

        // If one is assignable to the other, use the more general one
        if self.is_assignable_to(other) {
            return other.clone();
        }
        if other.is_assignable_to(self) {
            return self.clone();
        }

        // Null with wrapper -> wrapper
        if matches!(self, CelType::Null) && matches!(other, CelType::Wrapper(_)) {
            return other.clone();
        }
        if matches!(other, CelType::Null) && matches!(self, CelType::Wrapper(_)) {
            return self.clone();
        }

        // Null with message -> message
        if matches!(self, CelType::Null) && matches!(other, CelType::Message(_)) {
            return other.clone();
        }
        if matches!(other, CelType::Null) && matches!(self, CelType::Message(_)) {
            return self.clone();
        }

        // List common type
        if let (CelType::List(a), CelType::List(b)) = (self, other) {
            return CelType::List(Box::new(a.common_type(b)));
        }

        // Map common type
        if let (CelType::Map(ka, va), CelType::Map(kb, vb)) = (self, other) {
            return CelType::Map(Box::new(ka.common_type(kb)), Box::new(va.common_type(vb)));
        }

        // Abstract type common type (same name, common parameters)
        if let (CelType::Abstract(name_a, params_a), CelType::Abstract(name_b, params_b)) =
            (self, other)
        {
            if name_a == name_b && params_a.len() == params_b.len() {
                let common_params: Vec<CelType> = params_a
                    .iter()
                    .zip(params_b.iter())
                    .map(|(a, b)| a.common_type(b))
                    .collect();
                return CelType::Abstract(name_a.clone(), common_params);
            }
        }

        // Fall back to dyn
        CelType::Dyn
    }

    /// Substitute type parameters with concrete types.
    ///
    /// Used when resolving generic function overloads.
    pub fn substitute(&self, bindings: &std::collections::HashMap<Arc<str>, CelType>) -> CelType {
        match self {
            CelType::TypeParam(name) => bindings.get(name).cloned().unwrap_or(CelType::Dyn),
            CelType::List(elem) => CelType::List(Box::new(elem.substitute(bindings))),
            CelType::Map(key, val) => CelType::Map(
                Box::new(key.substitute(bindings)),
                Box::new(val.substitute(bindings)),
            ),
            CelType::Type(inner) => CelType::Type(Box::new(inner.substitute(bindings))),
            CelType::Function(params, ret) => CelType::Function(
                params.iter().map(|p| p.substitute(bindings)).collect(),
                Box::new(ret.substitute(bindings)),
            ),
            CelType::Abstract(name, params) => CelType::Abstract(
                name.clone(),
                params.iter().map(|p| p.substitute(bindings)).collect(),
            ),
            other => other.clone(),
        }
    }

    /// Get the element type if this is a list type.
    pub fn list_element(&self) -> Option<&CelType> {
        match self {
            CelType::List(elem) => Some(elem),
            _ => None,
        }
    }

    /// Get the key and value types if this is a map type.
    pub fn map_key_value(&self) -> Option<(&CelType, &CelType)> {
        match self {
            CelType::Map(key, val) => Some((key, val)),
            _ => None,
        }
    }
}

impl fmt::Display for CelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CelType::Dyn => write!(f, "dyn"),
            CelType::Null => write!(f, "null_type"),
            CelType::Bool => write!(f, "bool"),
            CelType::Int => write!(f, "int"),
            CelType::Uint => write!(f, "uint"),
            CelType::Double => write!(f, "double"),
            CelType::String => write!(f, "string"),
            CelType::Bytes => write!(f, "bytes"),
            CelType::Timestamp => write!(f, "google.protobuf.Timestamp"),
            CelType::Duration => write!(f, "google.protobuf.Duration"),
            CelType::Any => write!(f, "google.protobuf.Any"),
            CelType::List(elem) => write!(f, "list<{}>", elem),
            CelType::Map(key, val) => write!(f, "map<{}, {}>", key, val),
            CelType::Message(name) => write!(f, "{}", name),
            CelType::TypeParam(name) => write!(f, "{}", name),
            CelType::Type(inner) => write!(f, "type<{}>", inner),
            CelType::Wrapper(w) => write!(f, "wrapper<{:?}>", w),
            CelType::Function(params, ret) => {
                write!(f, "(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, ") -> {}", ret)
            }
            CelType::Abstract(name, params) => {
                write!(f, "{}", name)?;
                if !params.is_empty() {
                    write!(f, "<")?;
                    for (i, p) in params.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", p)?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            CelType::Error => write!(f, "*error*"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_assignability() {
        assert!(CelType::Int.is_assignable_to(&CelType::Int));
        assert!(CelType::Int.is_assignable_to(&CelType::Dyn));
        assert!(CelType::Dyn.is_assignable_to(&CelType::Int));
        assert!(!CelType::Int.is_assignable_to(&CelType::String));
    }

    #[test]
    fn test_null_assignability() {
        assert!(CelType::Null.is_assignable_to(&CelType::Wrapper(WrapperType::Int64)));
        assert!(CelType::Null.is_assignable_to(&CelType::Message("Foo".into())));
        assert!(!CelType::Null.is_assignable_to(&CelType::Int));
    }

    #[test]
    fn test_list_covariance() {
        let list_int = CelType::List(Box::new(CelType::Int));
        let list_dyn = CelType::List(Box::new(CelType::Dyn));

        assert!(list_int.is_assignable_to(&list_dyn));
        assert!(list_dyn.is_assignable_to(&list_int));
    }

    #[test]
    fn test_map_covariance() {
        let map_str_int = CelType::Map(Box::new(CelType::String), Box::new(CelType::Int));
        let map_str_dyn = CelType::Map(Box::new(CelType::String), Box::new(CelType::Dyn));

        assert!(map_str_int.is_assignable_to(&map_str_dyn));
    }

    #[test]
    fn test_common_type() {
        assert_eq!(CelType::Int.common_type(&CelType::Int), CelType::Int);
        assert_eq!(CelType::Int.common_type(&CelType::Dyn), CelType::Dyn);
        assert_eq!(CelType::Int.common_type(&CelType::String), CelType::Dyn);

        let list_int = CelType::List(Box::new(CelType::Int));
        let list_str = CelType::List(Box::new(CelType::String));
        assert_eq!(
            list_int.common_type(&list_str),
            CelType::List(Box::new(CelType::Dyn))
        );
    }

    #[test]
    fn test_substitute() {
        use std::collections::HashMap;

        let mut bindings = HashMap::new();
        bindings.insert(Arc::from("T"), CelType::Int);

        let list_t = CelType::List(Box::new(CelType::TypeParam("T".into())));
        assert_eq!(
            list_t.substitute(&bindings),
            CelType::List(Box::new(CelType::Int))
        );
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", CelType::Int), "int");
        assert_eq!(
            format!("{}", CelType::List(Box::new(CelType::String))),
            "list<string>"
        );
        assert_eq!(
            format!(
                "{}",
                CelType::Map(Box::new(CelType::String), Box::new(CelType::Int))
            ),
            "map<string, int>"
        );
    }
}

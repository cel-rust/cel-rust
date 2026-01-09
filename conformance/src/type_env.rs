//! Type environment conversion for conformance tests.
//!
//! This module converts proto Decl and Type messages to the CEL type checker types.

use cel::checker::{CelType, FunctionOverload, TypeEnv, WrapperType};
use std::sync::Arc;

use crate::proto::cel::expr::{
    r#type::{PrimitiveType, TypeKind, WellKnownType},
    Decl, Type,
};

/// Convert a proto Type message to a CelType.
pub fn proto_type_to_cel_type(proto_type: &Type) -> CelType {
    match &proto_type.type_kind {
        Some(TypeKind::Dyn(_)) => CelType::Dyn,
        Some(TypeKind::Null(_)) => CelType::Null,
        Some(TypeKind::Primitive(p)) => match PrimitiveType::try_from(*p) {
            Ok(PrimitiveType::Bool) => CelType::Bool,
            Ok(PrimitiveType::Int64) => CelType::Int,
            Ok(PrimitiveType::Uint64) => CelType::Uint,
            Ok(PrimitiveType::Double) => CelType::Double,
            Ok(PrimitiveType::String) => CelType::String,
            Ok(PrimitiveType::Bytes) => CelType::Bytes,
            _ => CelType::Dyn,
        },
        Some(TypeKind::Wrapper(w)) => match PrimitiveType::try_from(*w) {
            Ok(PrimitiveType::Bool) => CelType::Wrapper(WrapperType::Bool),
            Ok(PrimitiveType::Int64) => CelType::Wrapper(WrapperType::Int64),
            Ok(PrimitiveType::Uint64) => CelType::Wrapper(WrapperType::Uint64),
            Ok(PrimitiveType::Double) => CelType::Wrapper(WrapperType::Double),
            Ok(PrimitiveType::String) => CelType::Wrapper(WrapperType::String),
            Ok(PrimitiveType::Bytes) => CelType::Wrapper(WrapperType::Bytes),
            _ => CelType::Dyn,
        },
        Some(TypeKind::WellKnown(wk)) => match WellKnownType::try_from(*wk) {
            Ok(WellKnownType::Any) => CelType::Any,
            Ok(WellKnownType::Timestamp) => CelType::Timestamp,
            Ok(WellKnownType::Duration) => CelType::Duration,
            _ => CelType::Dyn,
        },
        Some(TypeKind::ListType(list)) => {
            let elem_type = list
                .elem_type
                .as_ref()
                .map(|t| proto_type_to_cel_type(t))
                .unwrap_or(CelType::Dyn);
            CelType::List(Box::new(elem_type))
        }
        Some(TypeKind::MapType(map)) => {
            let key_type = map
                .key_type
                .as_ref()
                .map(|t| proto_type_to_cel_type(t))
                .unwrap_or(CelType::Dyn);
            let value_type = map
                .value_type
                .as_ref()
                .map(|t| proto_type_to_cel_type(t))
                .unwrap_or(CelType::Dyn);
            CelType::Map(Box::new(key_type), Box::new(value_type))
        }
        Some(TypeKind::MessageType(name)) => CelType::Message(Arc::from(name.as_str())),
        Some(TypeKind::TypeParam(name)) => CelType::TypeParam(Arc::from(name.as_str())),
        Some(TypeKind::Type(inner)) => {
            let inner_type = proto_type_to_cel_type(inner);
            CelType::Type(Box::new(inner_type))
        }
        Some(TypeKind::Error(_)) => CelType::Error,
        Some(TypeKind::Function(func)) => {
            #[allow(clippy::redundant_closure)] // closure needed for Box<Type> deref coercion
            let result_type = func
                .result_type
                .as_ref()
                .map(|t| proto_type_to_cel_type(t))
                .unwrap_or(CelType::Dyn);
            let arg_types: Vec<CelType> =
                func.arg_types.iter().map(proto_type_to_cel_type).collect();
            CelType::Function(arg_types, Box::new(result_type))
        }
        Some(TypeKind::AbstractType(abs)) => {
            // Convert parameter types
            let params: Vec<CelType> = abs
                .parameter_types
                .iter()
                .map(proto_type_to_cel_type)
                .collect();
            CelType::Abstract(Arc::from(abs.name.as_str()), params)
        }
        None => CelType::Dyn,
    }
}

/// Convert a CelType to a proto Type message for comparison.
pub fn cel_type_to_proto_type(cel_type: &CelType) -> Type {
    let type_kind = match cel_type {
        CelType::Dyn => Some(TypeKind::Dyn(())),
        CelType::Null => Some(TypeKind::Null(0)), // NullValue::NullValue = 0
        CelType::Bool => Some(TypeKind::Primitive(PrimitiveType::Bool as i32)),
        CelType::Int => Some(TypeKind::Primitive(PrimitiveType::Int64 as i32)),
        CelType::Uint => Some(TypeKind::Primitive(PrimitiveType::Uint64 as i32)),
        CelType::Double => Some(TypeKind::Primitive(PrimitiveType::Double as i32)),
        CelType::String => Some(TypeKind::Primitive(PrimitiveType::String as i32)),
        CelType::Bytes => Some(TypeKind::Primitive(PrimitiveType::Bytes as i32)),
        CelType::Timestamp => Some(TypeKind::WellKnown(WellKnownType::Timestamp as i32)),
        CelType::Duration => Some(TypeKind::WellKnown(WellKnownType::Duration as i32)),
        CelType::Any => Some(TypeKind::WellKnown(WellKnownType::Any as i32)),
        CelType::List(elem) => Some(TypeKind::ListType(Box::new(
            crate::proto::cel::expr::r#type::ListType {
                elem_type: Some(Box::new(cel_type_to_proto_type(elem))),
            },
        ))),
        CelType::Map(key, val) => Some(TypeKind::MapType(Box::new(
            crate::proto::cel::expr::r#type::MapType {
                key_type: Some(Box::new(cel_type_to_proto_type(key))),
                value_type: Some(Box::new(cel_type_to_proto_type(val))),
            },
        ))),
        CelType::Message(name) => Some(TypeKind::MessageType(name.to_string())),
        CelType::TypeParam(name) => Some(TypeKind::TypeParam(name.to_string())),
        CelType::Type(inner) => Some(TypeKind::Type(Box::new(cel_type_to_proto_type(inner)))),
        CelType::Wrapper(w) => {
            let prim = match w {
                WrapperType::Bool => PrimitiveType::Bool,
                WrapperType::Int64 | WrapperType::Int32 => PrimitiveType::Int64,
                WrapperType::Uint64 | WrapperType::Uint32 => PrimitiveType::Uint64,
                WrapperType::Double | WrapperType::Float => PrimitiveType::Double,
                WrapperType::String => PrimitiveType::String,
                WrapperType::Bytes => PrimitiveType::Bytes,
            };
            Some(TypeKind::Wrapper(prim as i32))
        }
        CelType::Function(args, ret) => Some(TypeKind::Function(Box::new(
            crate::proto::cel::expr::r#type::FunctionType {
                result_type: Some(Box::new(cel_type_to_proto_type(ret))),
                arg_types: args.iter().map(cel_type_to_proto_type).collect(),
            },
        ))),
        CelType::Abstract(name, params) => Some(TypeKind::AbstractType(
            crate::proto::cel::expr::r#type::AbstractType {
                name: name.to_string(),
                parameter_types: params.iter().map(cel_type_to_proto_type).collect(),
            },
        )),
        CelType::Error => Some(TypeKind::Error(())),
    };

    Type { type_kind }
}

/// Build a TypeEnv from proto Decl messages.
pub fn build_type_env_from_decls(decls: &[Decl], container: &Option<String>) -> TypeEnv {
    let mut env = TypeEnv::standard();

    if let Some(c) = container {
        env.set_container(Some(c.as_str()));
    }

    for decl in decls {
        match &decl.decl_kind {
            Some(crate::proto::cel::expr::decl::DeclKind::Ident(ident)) => {
                if let Some(typ) = &ident.r#type {
                    let cel_type = proto_type_to_cel_type(typ);
                    env.add_variable(decl.name.as_str(), cel_type);
                }
            }
            Some(crate::proto::cel::expr::decl::DeclKind::Function(func)) => {
                for overload in &func.overloads {
                    let param_types: Vec<CelType> =
                        overload.params.iter().map(proto_type_to_cel_type).collect();

                    #[allow(clippy::redundant_closure)]
                    // closure needed for Box<Type> deref coercion
                    let result_type = overload
                        .result_type
                        .as_ref()
                        .map(|t| proto_type_to_cel_type(t))
                        .unwrap_or(CelType::Dyn);

                    let type_params: Vec<Arc<str>> = overload
                        .type_params
                        .iter()
                        .map(|s| Arc::from(s.as_str()))
                        .collect();

                    let func_overload = if overload.is_instance_function && !param_types.is_empty()
                    {
                        FunctionOverload::instance(
                            overload.overload_id.as_str(),
                            param_types[0].clone(),
                            param_types[1..].to_vec(),
                            result_type,
                        )
                        .with_type_params(type_params)
                    } else {
                        FunctionOverload::new(
                            overload.overload_id.as_str(),
                            param_types,
                            result_type,
                        )
                        .with_type_params(type_params)
                    };

                    env.add_function(decl.name.as_str(), func_overload);
                }
            }
            None => {}
        }
    }

    env
}

/// Compare two types for equality, handling type equivalence rules.
pub fn types_equal(expected: &Type, actual: &CelType) -> bool {
    let expected_cel = proto_type_to_cel_type(expected);

    // Simple equality check
    if &expected_cel == actual {
        return true;
    }

    // Handle dyn matching anything
    if expected_cel.is_dyn() || actual.is_dyn() {
        return true;
    }

    // Handle error type
    if expected_cel.is_error() || actual.is_error() {
        return true;
    }

    // Check assignability in both directions
    expected_cel.is_assignable_to(actual) || actual.is_assignable_to(&expected_cel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_type_conversion() {
        let proto_int = Type {
            type_kind: Some(TypeKind::Primitive(PrimitiveType::Int64 as i32)),
        };
        assert_eq!(proto_type_to_cel_type(&proto_int), CelType::Int);

        let proto_string = Type {
            type_kind: Some(TypeKind::Primitive(PrimitiveType::String as i32)),
        };
        assert_eq!(proto_type_to_cel_type(&proto_string), CelType::String);
    }

    #[test]
    fn test_list_type_conversion() {
        let proto_list = Type {
            type_kind: Some(TypeKind::ListType(Box::new(
                crate::proto::cel::expr::r#type::ListType {
                    elem_type: Some(Box::new(Type {
                        type_kind: Some(TypeKind::Primitive(PrimitiveType::Int64 as i32)),
                    })),
                },
            ))),
        };
        assert_eq!(
            proto_type_to_cel_type(&proto_list),
            CelType::List(Box::new(CelType::Int))
        );
    }

    #[test]
    fn test_roundtrip_conversion() {
        let types = vec![
            CelType::Int,
            CelType::String,
            CelType::Bool,
            CelType::Timestamp,
            CelType::List(Box::new(CelType::Int)),
            CelType::Map(Box::new(CelType::String), Box::new(CelType::Int)),
        ];

        for original in types {
            let proto = cel_type_to_proto_type(&original);
            let converted = proto_type_to_cel_type(&proto);
            assert_eq!(original, converted, "Roundtrip failed for {:?}", original);
        }
    }
}

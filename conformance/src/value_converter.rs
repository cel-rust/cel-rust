//! Value conversion from proto to CEL values.
//!
//! This is a simplified version for the conformance test suite.
//! Many proto types (Struct, Enum, Any messages) are not supported by the
//! current cel crate and will cause test failures.

use cel::objects::Value as CelValue;
use prost::Message;
use prost_types::Any;
use std::collections::HashMap;
use std::sync::Arc;

use crate::proto::cel::expr::Value as ProtoValue;

/// Converts a CEL spec protobuf Value to a cel-rust Value
pub(super) fn proto_value_to_cel_value(
    proto_value: &ProtoValue,
) -> Result<CelValue, ConversionError> {
    use cel::objects::{Key, Map, Value::*};

    match proto_value.kind.as_ref() {
        Some(crate::proto::cel::expr::value::Kind::NullValue(_)) => Ok(Null),
        Some(crate::proto::cel::expr::value::Kind::BoolValue(v)) => Ok(Bool(*v)),
        Some(crate::proto::cel::expr::value::Kind::Int64Value(v)) => Ok(Int(*v)),
        Some(crate::proto::cel::expr::value::Kind::Uint64Value(v)) => Ok(UInt(*v)),
        Some(crate::proto::cel::expr::value::Kind::DoubleValue(v)) => Ok(Float(*v)),
        Some(crate::proto::cel::expr::value::Kind::StringValue(v)) => {
            Ok(String(Arc::new(v.clone())))
        }
        Some(crate::proto::cel::expr::value::Kind::BytesValue(v)) => {
            Ok(Bytes(Arc::new(v.to_vec())))
        }
        Some(crate::proto::cel::expr::value::Kind::ListValue(list)) => {
            let mut values = Vec::new();
            for item in &list.values {
                values.push(proto_value_to_cel_value(item)?);
            }
            Ok(List(Arc::new(values)))
        }
        Some(crate::proto::cel::expr::value::Kind::MapValue(map)) => {
            let mut entries = HashMap::new();
            for entry in &map.entries {
                let key_proto = entry.key.as_ref().ok_or(ConversionError::MissingKey)?;
                let key_cel = proto_value_to_cel_value(key_proto)?;
                let value = proto_value_to_cel_value(
                    entry.value.as_ref().ok_or(ConversionError::MissingValue)?,
                )?;

                // Convert key to Key enum
                let key = match key_cel {
                    Int(i) => Key::Int(i),
                    UInt(u) => Key::Uint(u),
                    String(s) => Key::String(s),
                    Bool(b) => Key::Bool(b),
                    _ => return Err(ConversionError::UnsupportedKeyType),
                };
                entries.insert(key, value);
            }
            Ok(Map(Map {
                map: Arc::new(entries),
            }))
        }
        Some(crate::proto::cel::expr::value::Kind::EnumValue(enum_val)) => {
            // Enum type not supported in current cel crate - return as Int
            // Tests expecting Enum type will fail
            Ok(Int(enum_val.value as i64))
        }
        Some(crate::proto::cel::expr::value::Kind::ObjectValue(any)) => {
            let _decoded = convert_any_to_cel_value(any)?;
            Err(ConversionError::Unsupported(format!(
                "TODO: native support for protobuf messages is not supported yet (type_url: {})",
                any.type_url
            )))
        }
        Some(crate::proto::cel::expr::value::Kind::TypeValue(v)) => {
            // TypeValue is a string representing a type name
            Ok(String(Arc::new(v.clone())))
        }
        None => Err(ConversionError::EmptyValue),
    }
}

/// Converts a google.protobuf.Any message to a decoded protobuf message.
///
/// This helper is intentionally kept for future native protobuf-message support in CEL values.
pub(super) fn convert_any_to_cel_value(any: &Any) -> Result<Box<dyn Message>, ConversionError> {
    let type_name = any.type_url.rsplit('/').next().unwrap_or(&any.type_url);

    macro_rules! decode_message {
        ($ty:ty) => {
            <$ty>::decode(&any.value[..]).map_err(|e| {
                ConversionError::Unsupported(format!("Failed decoding {}: {}", type_name, e))
            })
        };
    }

    match type_name {
        "google.protobuf.BoolValue" => Ok(Box::new(decode_message!(BoolValueWkt)?)),
        "google.protobuf.BytesValue" => Ok(Box::new(decode_message!(BytesValueWkt)?)),
        "google.protobuf.DoubleValue" => Ok(Box::new(decode_message!(DoubleValueWkt)?)),
        "google.protobuf.FloatValue" => Ok(Box::new(decode_message!(FloatValueWkt)?)),
        "google.protobuf.Int32Value" => Ok(Box::new(decode_message!(Int32ValueWkt)?)),
        "google.protobuf.Int64Value" => Ok(Box::new(decode_message!(Int64ValueWkt)?)),
        "google.protobuf.StringValue" => Ok(Box::new(decode_message!(StringValueWkt)?)),
        "google.protobuf.UInt32Value" => Ok(Box::new(decode_message!(UInt32ValueWkt)?)),
        "google.protobuf.UInt64Value" => Ok(Box::new(decode_message!(UInt64ValueWkt)?)),
        "google.protobuf.Duration" => Ok(Box::new(decode_message!(prost_types::Duration)?)),
        "google.protobuf.Timestamp" => Ok(Box::new(decode_message!(prost_types::Timestamp)?)),
        "google.protobuf.ListValue" => Ok(Box::new(decode_message!(prost_types::ListValue)?)),
        "google.protobuf.Struct" => Ok(Box::new(decode_message!(prost_types::Struct)?)),
        "google.protobuf.Value" => Ok(Box::new(decode_message!(prost_types::Value)?)),
        "google.protobuf.Any" => {
            let inner_any = decode_message!(Any)?;
            convert_any_to_cel_value(&inner_any)
        }
        "cel.expr.conformance.proto2.TestAllTypes" => {
            let msg = decode_message!(crate::proto::cel::expr::conformance::proto2::TestAllTypes)?;
            Ok(Box::new(msg))
        }
        "cel.expr.conformance.proto2.NestedTestAllTypes" => {
            let msg =
                decode_message!(crate::proto::cel::expr::conformance::proto2::NestedTestAllTypes)?;
            Ok(Box::new(msg))
        }
        "cel.expr.conformance.proto2.TestAllTypes.NestedMessage" => {
            let msg = decode_message!(
                crate::proto::cel::expr::conformance::proto2::test_all_types::NestedMessage
            )?;
            Ok(Box::new(msg))
        }
        "cel.expr.conformance.proto3.TestAllTypes" => {
            let msg = decode_message!(crate::proto::cel::expr::conformance::proto3::TestAllTypes)?;
            Ok(Box::new(msg))
        }
        "cel.expr.conformance.proto3.NestedTestAllTypes" => {
            let msg =
                decode_message!(crate::proto::cel::expr::conformance::proto3::NestedTestAllTypes)?;
            Ok(Box::new(msg))
        }
        "cel.expr.conformance.proto3.TestAllTypes.NestedMessage" => {
            let msg = decode_message!(
                crate::proto::cel::expr::conformance::proto3::test_all_types::NestedMessage
            )?;
            Ok(Box::new(msg))
        }
        _ => Err(ConversionError::Unsupported(format!(
            "proto message type: {}",
            type_name
        ))),
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum ConversionError {
    #[error("Missing key in map entry")]
    MissingKey,
    #[error("Missing value in map entry")]
    MissingValue,
    #[error("Unsupported key type for map")]
    UnsupportedKeyType,
    #[error("Unsupported value type: {0}")]
    Unsupported(String),
    #[error("Empty value")]
    EmptyValue,
}

#[derive(Clone, PartialEq, ::prost::Message)]
struct BoolValueWkt {
    #[prost(bool, tag = "1")]
    value: bool,
}

#[derive(Clone, PartialEq, ::prost::Message)]
struct BytesValueWkt {
    #[prost(bytes = "vec", tag = "1")]
    value: Vec<u8>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
struct DoubleValueWkt {
    #[prost(double, tag = "1")]
    value: f64,
}

#[derive(Clone, PartialEq, ::prost::Message)]
struct FloatValueWkt {
    #[prost(float, tag = "1")]
    value: f32,
}

#[derive(Clone, PartialEq, ::prost::Message)]
struct Int32ValueWkt {
    #[prost(int32, tag = "1")]
    value: i32,
}

#[derive(Clone, PartialEq, ::prost::Message)]
struct Int64ValueWkt {
    #[prost(int64, tag = "1")]
    value: i64,
}

#[derive(Clone, PartialEq, ::prost::Message)]
struct StringValueWkt {
    #[prost(string, tag = "1")]
    value: String,
}

#[derive(Clone, PartialEq, ::prost::Message)]
struct UInt32ValueWkt {
    #[prost(uint32, tag = "1")]
    value: u32,
}

#[derive(Clone, PartialEq, ::prost::Message)]
struct UInt64ValueWkt {
    #[prost(uint64, tag = "1")]
    value: u64,
}

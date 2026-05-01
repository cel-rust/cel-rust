//! Value conversion from proto to CEL values.
//!
//! This is a simplified version for the conformance test suite.
//! Many proto types (Struct, Enum, Any messages) are not supported by the
//! current cel crate and will cause test failures.

use cel::objects::Value as CelValue;
use prost_types::Any;
use std::collections::HashMap;
use std::sync::Arc;

use crate::proto::cel::expr::Value as ProtoValue;

/// Converts a CEL spec protobuf Value to a cel-rust Value
pub fn proto_value_to_cel_value(proto_value: &ProtoValue) -> Result<CelValue, ConversionError> {
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
            convert_any_to_cel_value(any)
        }
        Some(crate::proto::cel::expr::value::Kind::TypeValue(v)) => {
            // TypeValue is a string representing a type name
            Ok(String(Arc::new(v.clone())))
        }
        None => Err(ConversionError::EmptyValue),
    }
}

/// Converts a google.protobuf.Any message to a CEL value.
/// Only handles wrapper types. Other proto messages will fail.
pub fn convert_any_to_cel_value(any: &Any) -> Result<CelValue, ConversionError> {
    use cel::objects::Value::*;

    let type_url = &any.type_url;

    // Helper to decode a varint
    fn decode_varint(bytes: &[u8]) -> Option<(u64, usize)> {
        let mut result = 0u64;
        let mut shift = 0;
        for (i, &byte) in bytes.iter().enumerate() {
            result |= ((byte & 0x7F) as u64) << shift;
            if (byte & 0x80) == 0 {
                return Some((result, i + 1));
            }
            shift += 7;
            if shift >= 64 {
                return None;
            }
        }
        None
    }

    // Helper to decode a fixed64 (double)
    fn decode_fixed64(bytes: &[u8]) -> Option<f64> {
        if bytes.len() < 8 {
            return None;
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&bytes[0..8]);
        Some(f64::from_le_bytes(buf))
    }

    // Helper to decode a fixed32 (float)
    fn decode_fixed32(bytes: &[u8]) -> Option<f32> {
        if bytes.len() < 4 {
            return None;
        }
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&bytes[0..4]);
        Some(f32::from_le_bytes(buf))
    }

    // Helper to decode a length-delimited string
    fn decode_string(bytes: &[u8]) -> Option<(std::string::String, usize)> {
        if let Some((len, len_bytes)) = decode_varint(bytes) {
            let len = len as usize;
            if bytes.len() >= len_bytes + len {
                if let Ok(s) =
                    std::string::String::from_utf8(bytes[len_bytes..len_bytes + len].to_vec())
                {
                    return Some((s, len_bytes + len));
                }
            }
        }
        None
    }

    // Decode wrapper types - they all have field number 1 with the value
    if type_url.contains("google.protobuf.BoolValue") {
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x08 {
                if let Some((bool_val, _)) = decode_varint(&any.value[1..]) {
                    return Ok(Bool(bool_val != 0));
                }
            }
        }
        // Empty wrapper = default value
        return Ok(Bool(false));
    } else if type_url.contains("google.protobuf.BytesValue") {
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x0A {
                if let Some((len, len_bytes)) = decode_varint(&any.value[1..]) {
                    let len = len as usize;
                    if any.value.len() >= 1 + len_bytes + len {
                        let bytes = any.value[1 + len_bytes..1 + len_bytes + len].to_vec();
                        return Ok(Bytes(Arc::new(bytes)));
                    }
                }
            }
        }
        return Ok(Bytes(Arc::new(Vec::new())));
    } else if type_url.contains("google.protobuf.DoubleValue") {
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x09 {
                if let Some(val) = decode_fixed64(&any.value[1..]) {
                    return Ok(Float(val));
                }
            }
        }
        return Ok(Float(0.0));
    } else if type_url.contains("google.protobuf.FloatValue") {
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x0D {
                if let Some(val) = decode_fixed32(&any.value[1..]) {
                    return Ok(Float(val as f64));
                }
            }
        }
        return Ok(Float(0.0));
    } else if type_url.contains("google.protobuf.Int32Value") {
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x08 {
                if let Some((val, _)) = decode_varint(&any.value[1..]) {
                    let val = val as i32;
                    return Ok(Int(val as i64));
                }
            }
        }
        return Ok(Int(0));
    } else if type_url.contains("google.protobuf.Int64Value") {
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x08 {
                if let Some((val, _)) = decode_varint(&any.value[1..]) {
                    let val = val as i64;
                    return Ok(Int(val));
                }
            }
        }
        return Ok(Int(0));
    } else if type_url.contains("google.protobuf.StringValue") {
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x0A {
                if let Some((s, _)) = decode_string(&any.value[1..]) {
                    return Ok(String(Arc::new(s)));
                }
            }
        }
        return Ok(String(Arc::new(std::string::String::new())));
    } else if type_url.contains("google.protobuf.UInt32Value")
        || type_url.contains("google.protobuf.UInt64Value")
    {
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x08 {
                if let Some((val, _)) = decode_varint(&any.value[1..]) {
                    return Ok(UInt(val));
                }
            }
        }
        return Ok(UInt(0));
    } else if type_url.contains("google.protobuf.Duration") {
        let mut seconds: i64 = 0;
        let mut nanos: i32 = 0;
        let mut pos = 0;

        while pos < any.value.len() {
            if let Some((field_and_type, len)) = decode_varint(&any.value[pos..]) {
                pos += len;
                let field_num = field_and_type >> 3;
                let wire_type = field_and_type & 0x07;

                if field_num == 1 && wire_type == 0 {
                    if let Some((val, len)) = decode_varint(&any.value[pos..]) {
                        seconds = val as i64;
                        pos += len;
                    } else {
                        break;
                    }
                } else if field_num == 2 && wire_type == 0 {
                    if let Some((val, len)) = decode_varint(&any.value[pos..]) {
                        nanos = val as i32;
                        pos += len;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        use chrono::Duration as ChronoDuration;
        let duration = ChronoDuration::seconds(seconds) + ChronoDuration::nanoseconds(nanos as i64);
        return Ok(Duration(duration));
    } else if type_url.contains("google.protobuf.Timestamp") {
        let mut seconds: i64 = 0;
        let mut nanos: i32 = 0;
        let mut pos = 0;

        while pos < any.value.len() {
            if let Some((field_and_type, len)) = decode_varint(&any.value[pos..]) {
                pos += len;
                let field_num = field_and_type >> 3;
                let wire_type = field_and_type & 0x07;

                if field_num == 1 && wire_type == 0 {
                    if let Some((val, len)) = decode_varint(&any.value[pos..]) {
                        seconds = val as i64;
                        pos += len;
                    } else {
                        break;
                    }
                } else if field_num == 2 && wire_type == 0 {
                    if let Some((val, len)) = decode_varint(&any.value[pos..]) {
                        nanos = val as i32;
                        pos += len;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        use chrono::{DateTime, TimeZone, Utc};
        let timestamp = Utc
            .timestamp_opt(seconds, nanos as u32)
            .single()
            .ok_or_else(|| ConversionError::Unsupported("Invalid timestamp values".to_string()))?;
        let fixed_offset = DateTime::from_naive_utc_and_offset(
            timestamp.naive_utc(),
            chrono::FixedOffset::east_opt(0).unwrap(),
        );
        return Ok(Timestamp(fixed_offset));
    }

    // Handle google.protobuf.ListValue
    if type_url.contains("google.protobuf.ListValue") {
        use prost::Message;
        if let Ok(list_value) = prost_types::ListValue::decode(&any.value[..]) {
            let mut values = Vec::new();
            for item in &list_value.values {
                values.push(convert_protobuf_value_to_cel(item)?);
            }
            return Ok(List(Arc::new(values)));
        }
    }

    // Handle google.protobuf.Struct
    if type_url.contains("google.protobuf.Struct") {
        use prost::Message;
        if let Ok(struct_val) = prost_types::Struct::decode(&any.value[..]) {
            let mut map_entries = HashMap::new();
            for (key, value) in &struct_val.fields {
                let cel_value = convert_protobuf_value_to_cel(value)?;
                map_entries.insert(cel::objects::Key::String(Arc::new(key.clone())), cel_value);
            }
            return Ok(Map(cel::objects::Map {
                map: Arc::new(map_entries),
            }));
        }
    }

    // Handle google.protobuf.Value
    if type_url.contains("google.protobuf.Value") {
        use prost::Message;
        if let Ok(value) = prost_types::Value::decode(&any.value[..]) {
            return convert_protobuf_value_to_cel(&value);
        }
    }

    // Handle nested Any messages
    use prost::Message;
    if type_url.contains("google.protobuf.Any") {
        if let Ok(inner_any) = Any::decode(&any.value[..]) {
            return convert_any_to_cel_value(&inner_any);
        }
    }

    // Extract the type name for error message
    let type_name = if let Some(last_slash) = type_url.rfind('/') {
        &type_url[last_slash + 1..]
    } else {
        type_url
    };

    // Proto message types (Struct, TestAllTypes, etc.) are not supported
    // Tests requiring these will fail
    Err(ConversionError::Unsupported(format!(
        "proto message type: {} (Struct type not available in cel crate)",
        type_name
    )))
}

/// Convert a google.protobuf.Value to a CEL Value
fn convert_protobuf_value_to_cel(value: &prost_types::Value) -> Result<CelValue, ConversionError> {
    use cel::objects::{Key, Map, Value::*};
    use prost_types::value::Kind;

    match &value.kind {
        Some(Kind::NullValue(_)) => Ok(Null),
        Some(Kind::NumberValue(n)) => Ok(Float(*n)),
        Some(Kind::StringValue(s)) => Ok(String(Arc::new(s.clone()))),
        Some(Kind::BoolValue(b)) => Ok(Bool(*b)),
        Some(Kind::StructValue(s)) => {
            let mut map_entries = HashMap::new();
            for (key, val) in &s.fields {
                let cel_val = convert_protobuf_value_to_cel(val)?;
                map_entries.insert(Key::String(Arc::new(key.clone())), cel_val);
            }
            Ok(Map(Map {
                map: Arc::new(map_entries),
            }))
        }
        Some(Kind::ListValue(l)) => {
            let mut list_items = Vec::new();
            for item in &l.values {
                list_items.push(convert_protobuf_value_to_cel(item)?);
            }
            Ok(List(Arc::new(list_items)))
        }
        None => Ok(Null),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
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

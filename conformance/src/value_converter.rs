use cel::objects::Value as CelValue;
use prost_types::Any;
use std::collections::HashMap;
use std::sync::Arc;

use crate::proto::cel::expr::Value as ProtoValue;

/// Converts a CEL spec protobuf Value to a cel-rust Value
pub fn proto_value_to_cel_value(proto_value: &ProtoValue) -> Result<CelValue, ConversionError> {
    use cel::objects::{Key, Map, Value::*};
    use std::sync::Arc;

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
            // Enum values include their type name for type() introspection
            Ok(Enum(
                enum_val.value as i64,
                Arc::new(enum_val.r#type.clone()),
            ))
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
/// Handles wrapper types and converts other messages to Structs.
pub fn convert_any_to_cel_value(any: &Any) -> Result<CelValue, ConversionError> {
    use cel::objects::Value::*;

    // Try to decode as wrapper types first
    // Wrapper types should be unwrapped to their inner value
    let type_url = &any.type_url;

    // Wrapper types in protobuf are simple: they have a single field named "value"
    // We can manually decode them from the wire format
    // Wire format: field_number (1 byte varint) + wire_type + value

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
        // Field 1: bool value (wire type 0 = varint)
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x08 {
                // field 1, wire type 0
                if let Some((bool_val, _)) = decode_varint(&any.value[1..]) {
                    return Ok(Bool(bool_val != 0));
                }
            }
        }
    } else if type_url.contains("google.protobuf.BytesValue") {
        // Field 1: bytes value (wire type 2 = length-delimited)
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x0A {
                // field 1, wire type 2
                if let Some((len, len_bytes)) = decode_varint(&any.value[1..]) {
                    let len = len as usize;
                    if any.value.len() >= 1 + len_bytes + len {
                        let bytes = any.value[1 + len_bytes..1 + len_bytes + len].to_vec();
                        return Ok(Bytes(Arc::new(bytes)));
                    }
                }
            }
        }
    } else if type_url.contains("google.protobuf.DoubleValue") {
        // Field 1: double value (wire type 1 = fixed64)
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x09 {
                // field 1, wire type 1
                if let Some(val) = decode_fixed64(&any.value[1..]) {
                    return Ok(Float(val));
                }
            }
        }
    } else if type_url.contains("google.protobuf.FloatValue") {
        // Field 1: float value (wire type 5 = fixed32)
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x0D {
                // field 1, wire type 5
                if let Some(val) = decode_fixed32(&any.value[1..]) {
                    return Ok(Float(val as f64));
                }
            }
        }
    } else if type_url.contains("google.protobuf.Int32Value") {
        // Field 1: int32 value (wire type 0 = varint, signed but not zigzag)
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x08 {
                // field 1, wire type 0
                if let Some((val, _)) = decode_varint(&any.value[1..]) {
                    // Convert to signed i32 (two's complement)
                    let val = val as i32;
                    return Ok(Int(val as i64));
                }
            }
        }
    } else if type_url.contains("google.protobuf.Int64Value") {
        // Field 1: int64 value (wire type 0 = varint, signed but not zigzag)
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x08 {
                // field 1, wire type 0
                if let Some((val, _)) = decode_varint(&any.value[1..]) {
                    // Convert to signed i64 (two's complement)
                    let val = val as i64;
                    return Ok(Int(val));
                }
            }
        }
    } else if type_url.contains("google.protobuf.StringValue") {
        // Field 1: string value (wire type 2 = length-delimited)
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x0A {
                // field 1, wire type 2
                if let Some((s, _)) = decode_string(&any.value[1..]) {
                    return Ok(String(Arc::new(s)));
                }
            }
        }
    } else if type_url.contains("google.protobuf.UInt32Value") {
        // Field 1: uint32 value (wire type 0 = varint)
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x08 {
                // field 1, wire type 0
                if let Some((val, _)) = decode_varint(&any.value[1..]) {
                    return Ok(UInt(val));
                }
            }
        }
    } else if type_url.contains("google.protobuf.UInt64Value") {
        // Field 1: uint64 value (wire type 0 = varint)
        if let Some((field_and_type, _)) = decode_varint(&any.value) {
            if field_and_type == 0x08 {
                // field 1, wire type 0
                if let Some((val, _)) = decode_varint(&any.value[1..]) {
                    return Ok(UInt(val));
                }
            }
        }
    } else if type_url.contains("google.protobuf.Duration") {
        // google.protobuf.Duration has two fields:
        // - field 1: seconds (int64, wire type 0 = varint)
        // - field 2: nanos (int32, wire type 0 = varint)
        let mut seconds: i64 = 0;
        let mut nanos: i32 = 0;
        let mut pos = 0;

        while pos < any.value.len() {
            if let Some((field_and_type, len)) = decode_varint(&any.value[pos..]) {
                pos += len;
                let field_num = field_and_type >> 3;
                let wire_type = field_and_type & 0x07;

                if field_num == 1 && wire_type == 0 {
                    // seconds field
                    if let Some((val, len)) = decode_varint(&any.value[pos..]) {
                        seconds = val as i64;
                        pos += len;
                    } else {
                        break;
                    }
                } else if field_num == 2 && wire_type == 0 {
                    // nanos field
                    if let Some((val, len)) = decode_varint(&any.value[pos..]) {
                        nanos = val as i32;
                        pos += len;
                    } else {
                        break;
                    }
                } else {
                    // Unknown field, skip it
                    break;
                }
            } else {
                break;
            }
        }

        // Convert to CEL Duration
        use chrono::Duration as ChronoDuration;
        let duration = ChronoDuration::seconds(seconds) + ChronoDuration::nanoseconds(nanos as i64);
        return Ok(Duration(duration));
    } else if type_url.contains("google.protobuf.Timestamp") {
        // google.protobuf.Timestamp has two fields:
        // - field 1: seconds (int64, wire type 0 = varint)
        // - field 2: nanos (int32, wire type 0 = varint)
        let mut seconds: i64 = 0;
        let mut nanos: i32 = 0;
        let mut pos = 0;

        while pos < any.value.len() {
            if let Some((field_and_type, len)) = decode_varint(&any.value[pos..]) {
                pos += len;
                let field_num = field_and_type >> 3;
                let wire_type = field_and_type & 0x07;

                if field_num == 1 && wire_type == 0 {
                    // seconds field
                    if let Some((val, len)) = decode_varint(&any.value[pos..]) {
                        seconds = val as i64;
                        pos += len;
                    } else {
                        break;
                    }
                } else if field_num == 2 && wire_type == 0 {
                    // nanos field
                    if let Some((val, len)) = decode_varint(&any.value[pos..]) {
                        nanos = val as i32;
                        pos += len;
                    } else {
                        break;
                    }
                } else {
                    // Unknown field, skip it
                    break;
                }
            } else {
                break;
            }
        }

        // Convert to CEL Timestamp
        use chrono::{DateTime, TimeZone, Utc};
        let timestamp = Utc
            .timestamp_opt(seconds, nanos as u32)
            .single()
            .ok_or_else(|| ConversionError::Unsupported("Invalid timestamp values".to_string()))?;
        // Convert to FixedOffset (UTC = +00:00)
        let fixed_offset = DateTime::from_naive_utc_and_offset(
            timestamp.naive_utc(),
            chrono::FixedOffset::east_opt(0).unwrap(),
        );
        return Ok(Timestamp(fixed_offset));
    }

    // For other proto messages, try to decode them and convert to Struct
    // Extract the type name from the type_url (format: type.googleapis.com/packagename.MessageName)
    let type_name = if let Some(last_slash) = type_url.rfind('/') {
        &type_url[last_slash + 1..]
    } else {
        type_url
    };

    // Handle google.protobuf.ListValue - return a list
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

    // Handle google.protobuf.Struct - return a map
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

    // Handle google.protobuf.Value - return the appropriate CEL value
    if type_url.contains("google.protobuf.Value") {
        use prost::Message;
        if let Ok(value) = prost_types::Value::decode(&any.value[..]) {
            return convert_protobuf_value_to_cel(&value);
        }
    }

    // Handle nested Any messages (recursively unpack)
    use prost::Message;
    if type_url.contains("google.protobuf.Any") {
        if let Ok(inner_any) = Any::decode(&any.value[..]) {
            // Recursively unpack the inner Any
            return convert_any_to_cel_value(&inner_any);
        }
    }

    // Try to decode as TestAllTypes (proto2 or proto3)
    // IMPORTANT: Check for nested message types FIRST, before checking for TestAllTypes
    // because "TestAllTypes.NestedMessage" contains "TestAllTypes"
    if type_url.contains("cel.expr.conformance.proto3.TestAllTypes.NestedMessage") {
        use cel::objects::{Struct, Value::*};
        use prost::Message;
        if let Ok(nested) =
            crate::proto::cel::expr::conformance::proto3::test_all_types::NestedMessage::decode(
                &any.value[..],
            )
        {
            let mut nested_fields = HashMap::new();
            nested_fields.insert("bb".to_string(), Int(nested.bb as i64));
            return Ok(Struct(Struct {
                type_name: Arc::new(
                    "cel.expr.conformance.proto3.TestAllTypes.NestedMessage".to_string(),
                ),
                fields: Arc::new(nested_fields),
            }));
        }
    } else if type_url.contains("cel.expr.conformance.proto2.TestAllTypes.NestedMessage") {
        use cel::objects::{Struct, Value::*};
        use prost::Message;
        if let Ok(nested) =
            crate::proto::cel::expr::conformance::proto2::test_all_types::NestedMessage::decode(
                &any.value[..],
            )
        {
            let mut nested_fields = HashMap::new();
            nested_fields.insert("bb".to_string(), Int(nested.bb.unwrap_or(0) as i64));
            return Ok(Struct(Struct {
                type_name: Arc::new(
                    "cel.expr.conformance.proto2.TestAllTypes.NestedMessage".to_string(),
                ),
                fields: Arc::new(nested_fields),
            }));
        }
    } else if type_url.contains("cel.expr.conformance.proto3.TestAllTypes") {
        if let Ok(msg) =
            crate::proto::cel::expr::conformance::proto3::TestAllTypes::decode(&any.value[..])
        {
            return convert_test_all_types_proto3_to_struct_with_bytes(&msg, &any.value);
        }
    } else if type_url.contains("cel.expr.conformance.proto2.TestAllTypes") {
        if let Ok(msg) =
            crate::proto::cel::expr::conformance::proto2::TestAllTypes::decode(&any.value[..])
        {
            return convert_test_all_types_proto2_to_struct(&msg, &any.value);
        }
    }

    // For other proto messages, return an error for now
    // We can extend this to handle more message types as needed
    Err(ConversionError::Unsupported(format!(
        "proto message type: {} (not yet supported)",
        type_name
    )))
}

/// Extract extension fields from a protobuf message's wire format.
/// Extension fields have field numbers >= 1000.
fn extract_extension_fields(
    encoded_msg: &[u8],
    fields: &mut HashMap<String, CelValue>,
) -> Result<(), ConversionError> {
    use cel::proto_compare::{field_value_to_cel, parse_proto_wire_format};

    // Parse wire format to get all fields
    let field_map = match parse_proto_wire_format(encoded_msg) {
        Some(map) => map,
        None => return Ok(()),
    };

    // Helper to convert embedded TestAllTypes message bytes to CEL Struct
    fn convert_embedded_test_all_types(bytes: &[u8]) -> CelValue {
        use cel::objects::{Struct, Value::*};
        use prost::Message;

        // Try to decode as TestAllTypes
        if let Ok(msg) = crate::proto::cel::expr::conformance::proto2::TestAllTypes::decode(bytes) {
            // Create a struct with wrapper fields set to Null (proto2 defaults)
            let mut struct_fields = HashMap::new();

            // Add wrapper fields as Null (proto2 semantics - absent wrappers are null)
            struct_fields.insert("single_int32_wrapper".to_string(), Null);
            struct_fields.insert("single_int64_wrapper".to_string(), Null);
            struct_fields.insert("single_uint32_wrapper".to_string(), Null);
            struct_fields.insert("single_uint64_wrapper".to_string(), Null);
            struct_fields.insert("single_float_wrapper".to_string(), Null);
            struct_fields.insert("single_double_wrapper".to_string(), Null);
            struct_fields.insert("single_bool_wrapper".to_string(), Null);
            struct_fields.insert("single_string_wrapper".to_string(), Null);
            struct_fields.insert("single_bytes_wrapper".to_string(), Null);

            // Add the single_bool field - proto2 has [default = true] for this field
            if msg.single_bool.is_some() {
                struct_fields.insert("single_bool".to_string(), Bool(msg.single_bool.unwrap()));
            } else {
                // Proto2 default for single_bool is true (from [default = true] in schema)
                struct_fields.insert("single_bool".to_string(), Bool(true));
            }

            // Add single_int64 if set
            if let Some(v) = msg.single_int64 {
                struct_fields.insert("single_int64".to_string(), Int(v));
            }

            Struct(Struct {
                type_name: Arc::new("cel.expr.conformance.proto2.TestAllTypes".to_string()),
                fields: Arc::new(struct_fields),
            })
        } else {
            // Fallback: return empty struct if decode fails
            Struct(Struct {
                type_name: Arc::new("cel.expr.conformance.proto2.TestAllTypes".to_string()),
                fields: Arc::new({
                    let mut fields = HashMap::new();
                    fields.insert("single_int32_wrapper".to_string(), Null);
                    fields.insert("single_int64_wrapper".to_string(), Null);
                    fields.insert("single_uint32_wrapper".to_string(), Null);
                    fields.insert("single_uint64_wrapper".to_string(), Null);
                    fields.insert("single_float_wrapper".to_string(), Null);
                    fields.insert("single_double_wrapper".to_string(), Null);
                    fields.insert("single_bool_wrapper".to_string(), Null);
                    fields.insert("single_string_wrapper".to_string(), Null);
                    fields.insert("single_bytes_wrapper".to_string(), Null);
                    // Proto2 default for single_bool is true (from [default = true] in schema)
                    fields.insert("single_bool".to_string(), Bool(true));
                    fields
                }),
            })
        }
    }

    // Process extension fields (field numbers >= 1000)
    for (field_num, values) in field_map {
        if field_num >= 1000 {
            // Map field number to fully qualified extension name
            let ext_name = match field_num {
                1000 => "cel.expr.conformance.proto2.int32_ext",
                1001 => "cel.expr.conformance.proto2.nested_ext",
                1002 => "cel.expr.conformance.proto2.test_all_types_ext",
                1003 => "cel.expr.conformance.proto2.nested_enum_ext",
                1004 => "cel.expr.conformance.proto2.repeated_test_all_types",
                1005 => "cel.expr.conformance.proto2.Proto2ExtensionScopedMessage.int64_ext",
                1006 => "cel.expr.conformance.proto2.Proto2ExtensionScopedMessage.message_scoped_nested_ext",
                1007 => "cel.expr.conformance.proto2.Proto2ExtensionScopedMessage.nested_enum_ext",
                1008 => "cel.expr.conformance.proto2.Proto2ExtensionScopedMessage.message_scoped_repeated_test_all_types",
                _ => continue, // Unknown extension
            };

            // Handle based on extension type
            match field_num {
                // Repeated message extensions
                1004 | 1008 => {
                    let list_values: Vec<CelValue> = values
                        .iter()
                        .filter_map(|v| {
                            if let cel::proto_compare::FieldValue::LengthDelimited(bytes) = v {
                                Some(convert_embedded_test_all_types(bytes))
                            } else {
                                None
                            }
                        })
                        .collect();
                    fields.insert(ext_name.to_string(), CelValue::List(Arc::new(list_values)));
                }
                // Singular message extensions (nested_ext, test_all_types_ext, message_scoped_nested_ext)
                1001 | 1002 | 1006 => {
                    if let Some(cel::proto_compare::FieldValue::LengthDelimited(bytes)) =
                        values.first()
                    {
                        let cel_value = convert_embedded_test_all_types(bytes);
                        fields.insert(ext_name.to_string(), cel_value);
                    }
                }
                // Enum extensions
                1003 | 1007 => {
                    if let Some(first_value) = values.first() {
                        let cel_value = field_value_to_cel(first_value);
                        // Convert to Enum type for proper CEL handling
                        let enum_value = match cel_value {
                            CelValue::Int(i) => CelValue::Enum(
                                i,
                                Arc::new(
                                    "cel.expr.conformance.proto2.TestAllTypes.NestedEnum"
                                        .to_string(),
                                ),
                            ),
                            other => other,
                        };
                        fields.insert(ext_name.to_string(), enum_value);
                    }
                }
                // Scalar extensions (int32_ext, int64_ext)
                _ => {
                    if let Some(first_value) = values.first() {
                        let cel_value = field_value_to_cel(first_value);
                        fields.insert(ext_name.to_string(), cel_value);
                    }
                }
            }
        }
    }

    Ok(())
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
            // Convert Struct to Map
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
            // Convert ListValue to List
            let mut list_items = Vec::new();
            for item in &l.values {
                list_items.push(convert_protobuf_value_to_cel(item)?);
            }
            Ok(List(Arc::new(list_items)))
        }
        None => Ok(Null),
    }
}

/// Parse oneof field from wire format if it's present but not decoded by prost
/// Returns (field_name, cel_value) if found
fn parse_oneof_from_wire_format(
    wire_bytes: &[u8],
) -> Result<Option<(String, CelValue)>, ConversionError> {
    use cel::proto_compare::parse_proto_wire_format;
    use prost::Message;

    // Parse wire format to get all fields
    let field_map = match parse_proto_wire_format(wire_bytes) {
        Some(map) => map,
        None => return Ok(None),
    };

    // Check for oneof field 400 (oneof_type - NestedTestAllTypes)
    if let Some(values) = field_map.get(&400) {
        // Field 400 is a length-delimited message (NestedTestAllTypes)
        if let Some(cel::proto_compare::FieldValue::LengthDelimited(bytes)) = values.first() {
            // Decode as NestedTestAllTypes
            if let Ok(nested) =
                crate::proto::cel::expr::conformance::proto3::NestedTestAllTypes::decode(&bytes[..])
            {
                // Convert NestedTestAllTypes to struct
                let mut nested_fields = HashMap::new();

                // Handle child field (recursive NestedTestAllTypes)
                if let Some(ref child) = nested.child {
                    let mut child_fields = HashMap::new();
                    if let Some(ref payload) = child.payload {
                        let payload_struct = convert_test_all_types_proto3_to_struct(payload)?;
                        child_fields.insert("payload".to_string(), payload_struct);
                    }
                    let child_struct = CelValue::Struct(cel::objects::Struct {
                        type_name: Arc::new(
                            "cel.expr.conformance.proto3.NestedTestAllTypes".to_string(),
                        ),
                        fields: Arc::new(child_fields),
                    });
                    nested_fields.insert("child".to_string(), child_struct);
                }

                // Handle payload field (TestAllTypes)
                if let Some(ref payload) = nested.payload {
                    let payload_struct = convert_test_all_types_proto3_to_struct(payload)?;
                    nested_fields.insert("payload".to_string(), payload_struct);
                }

                let nested_struct = CelValue::Struct(cel::objects::Struct {
                    type_name: Arc::new(
                        "cel.expr.conformance.proto3.NestedTestAllTypes".to_string(),
                    ),
                    fields: Arc::new(nested_fields),
                });
                return Ok(Some(("oneof_type".to_string(), nested_struct)));
            }
        }
    }

    // Check for oneof field 401 (oneof_msg - NestedMessage)
    if let Some(values) = field_map.get(&401) {
        if let Some(cel::proto_compare::FieldValue::LengthDelimited(bytes)) = values.first() {
            if let Ok(nested) =
                crate::proto::cel::expr::conformance::proto3::test_all_types::NestedMessage::decode(
                    &bytes[..],
                )
            {
                let mut nested_fields = HashMap::new();
                nested_fields.insert("bb".to_string(), CelValue::Int(nested.bb as i64));
                let nested_struct = CelValue::Struct(cel::objects::Struct {
                    type_name: Arc::new("cel.expr.conformance.proto3.NestedMessage".to_string()),
                    fields: Arc::new(nested_fields),
                });
                return Ok(Some(("oneof_msg".to_string(), nested_struct)));
            }
        }
    }

    // Check for oneof field 402 (oneof_bool - bool)
    if let Some(values) = field_map.get(&402) {
        if let Some(cel::proto_compare::FieldValue::Varint(v)) = values.first() {
            return Ok(Some(("oneof_bool".to_string(), CelValue::Bool(*v != 0))));
        }
    }

    Ok(None)
}

/// Convert a proto3 TestAllTypes message to a CEL Struct (wrapper without bytes)
fn convert_test_all_types_proto3_to_struct(
    msg: &crate::proto::cel::expr::conformance::proto3::TestAllTypes,
) -> Result<CelValue, ConversionError> {
    use prost::Message;
    let mut bytes = Vec::new();
    msg.encode(&mut bytes)
        .map_err(|e| ConversionError::Unsupported(format!("Failed to encode: {}", e)))?;
    convert_test_all_types_proto3_to_struct_with_bytes(msg, &bytes)
}

/// Convert a proto3 TestAllTypes message to a CEL Struct
fn convert_test_all_types_proto3_to_struct_with_bytes(
    msg: &crate::proto::cel::expr::conformance::proto3::TestAllTypes,
    original_bytes: &[u8],
) -> Result<CelValue, ConversionError> {
    use cel::objects::{Struct, Value::*};
    use std::sync::Arc;

    let mut fields = HashMap::new();

    // Wrapper types are already decoded by prost - convert them to CEL values or Null
    // Unset wrapper fields should map to Null, not be missing from the struct
    fields.insert(
        "single_bool_wrapper".to_string(),
        msg.single_bool_wrapper.map(Bool).unwrap_or(Null),
    );
    fields.insert(
        "single_bytes_wrapper".to_string(),
        msg.single_bytes_wrapper
            .as_ref()
            .map(|v| Bytes(Arc::new(v.clone())))
            .unwrap_or(Null),
    );
    fields.insert(
        "single_double_wrapper".to_string(),
        msg.single_double_wrapper.map(Float).unwrap_or(Null),
    );
    fields.insert(
        "single_float_wrapper".to_string(),
        msg.single_float_wrapper
            .map(|v| Float(v as f64))
            .unwrap_or(Null),
    );
    fields.insert(
        "single_int32_wrapper".to_string(),
        msg.single_int32_wrapper
            .map(|v| Int(v as i64))
            .unwrap_or(Null),
    );
    fields.insert(
        "single_int64_wrapper".to_string(),
        msg.single_int64_wrapper.map(Int).unwrap_or(Null),
    );
    fields.insert(
        "single_string_wrapper".to_string(),
        msg.single_string_wrapper
            .as_ref()
            .map(|v| String(Arc::new(v.clone())))
            .unwrap_or(Null),
    );
    fields.insert(
        "single_uint32_wrapper".to_string(),
        msg.single_uint32_wrapper
            .map(|v| UInt(v as u64))
            .unwrap_or(Null),
    );
    fields.insert(
        "single_uint64_wrapper".to_string(),
        msg.single_uint64_wrapper.map(UInt).unwrap_or(Null),
    );

    // Add other fields
    fields.insert("single_bool".to_string(), Bool(msg.single_bool));
    fields.insert(
        "single_string".to_string(),
        String(Arc::new(msg.single_string.clone())),
    );
    fields.insert(
        "single_bytes".to_string(),
        Bytes(Arc::new(msg.single_bytes.as_ref().to_vec())),
    );
    fields.insert("single_int32".to_string(), Int(msg.single_int32 as i64));
    fields.insert("single_int64".to_string(), Int(msg.single_int64));
    fields.insert("single_uint32".to_string(), UInt(msg.single_uint32 as u64));
    fields.insert("single_uint64".to_string(), UInt(msg.single_uint64));
    fields.insert("single_sint32".to_string(), Int(msg.single_sint32 as i64));
    fields.insert("single_sint64".to_string(), Int(msg.single_sint64));
    fields.insert(
        "single_fixed32".to_string(),
        UInt(msg.single_fixed32 as u64),
    );
    fields.insert("single_fixed64".to_string(), UInt(msg.single_fixed64));
    fields.insert(
        "single_sfixed32".to_string(),
        Int(msg.single_sfixed32 as i64),
    );
    fields.insert("single_sfixed64".to_string(), Int(msg.single_sfixed64));
    fields.insert("single_float".to_string(), Float(msg.single_float as f64));
    fields.insert("single_double".to_string(), Float(msg.single_double));

    // Handle standalone_enum field (proto3 enums are not optional)
    fields.insert(
        "standalone_enum".to_string(),
        Enum(
            msg.standalone_enum as i64,
            Arc::new("cel.expr.conformance.proto3.TestAllTypes.NestedEnum".to_string()),
        ),
    );

    // Handle reserved keyword fields (fields 500-516)
    // These will be filtered out later, but we need to include them first
    // in case the test data sets them
    fields.insert("as".to_string(), Bool(msg.r#as));
    fields.insert("break".to_string(), Bool(msg.r#break));
    fields.insert("const".to_string(), Bool(msg.r#const));
    fields.insert("continue".to_string(), Bool(msg.r#continue));
    fields.insert("else".to_string(), Bool(msg.r#else));
    fields.insert("for".to_string(), Bool(msg.r#for));
    fields.insert("function".to_string(), Bool(msg.r#function));
    fields.insert("if".to_string(), Bool(msg.r#if));
    fields.insert("import".to_string(), Bool(msg.r#import));
    fields.insert("let".to_string(), Bool(msg.r#let));
    fields.insert("loop".to_string(), Bool(msg.r#loop));
    fields.insert("package".to_string(), Bool(msg.r#package));
    fields.insert("namespace".to_string(), Bool(msg.r#namespace));
    fields.insert("return".to_string(), Bool(msg.r#return));
    fields.insert("var".to_string(), Bool(msg.r#var));
    fields.insert("void".to_string(), Bool(msg.r#void));
    fields.insert("while".to_string(), Bool(msg.r#while));

    // Handle oneof field (kind)
    if let Some(ref kind) = msg.kind {
        use crate::proto::cel::expr::conformance::proto3::test_all_types::Kind;
        match kind {
            Kind::OneofType(nested) => {
                // Convert NestedTestAllTypes - has child and payload fields
                let mut nested_fields = HashMap::new();

                // Handle child field (recursive NestedTestAllTypes)
                if let Some(ref child) = nested.child {
                    // Recursively convert child (simplified for now - just handle payload)
                    let mut child_fields = HashMap::new();
                    if let Some(ref payload) = child.payload {
                        let payload_struct = convert_test_all_types_proto3_to_struct(payload)?;
                        child_fields.insert("payload".to_string(), payload_struct);
                    }
                    let child_struct = Struct(Struct {
                        type_name: Arc::new(
                            "cel.expr.conformance.proto3.NestedTestAllTypes".to_string(),
                        ),
                        fields: Arc::new(child_fields),
                    });
                    nested_fields.insert("child".to_string(), child_struct);
                }

                // Handle payload field (TestAllTypes)
                if let Some(ref payload) = nested.payload {
                    let payload_struct = convert_test_all_types_proto3_to_struct(payload)?;
                    nested_fields.insert("payload".to_string(), payload_struct);
                }

                let nested_struct = Struct(Struct {
                    type_name: Arc::new(
                        "cel.expr.conformance.proto3.NestedTestAllTypes".to_string(),
                    ),
                    fields: Arc::new(nested_fields),
                });
                fields.insert("oneof_type".to_string(), nested_struct);
            }
            Kind::OneofMsg(nested) => {
                // Convert NestedMessage to struct
                let mut nested_fields = HashMap::new();
                nested_fields.insert("bb".to_string(), Int(nested.bb as i64));
                let nested_struct = Struct(Struct {
                    type_name: Arc::new("cel.expr.conformance.proto3.NestedMessage".to_string()),
                    fields: Arc::new(nested_fields),
                });
                fields.insert("oneof_msg".to_string(), nested_struct);
            }
            Kind::OneofBool(b) => {
                fields.insert("oneof_bool".to_string(), Bool(*b));
            }
        }
    }

    // Handle optional message fields (well-known types)
    if let Some(ref struct_val) = msg.single_struct {
        // Convert google.protobuf.Struct to CEL Map
        let mut map_entries = HashMap::new();
        for (key, value) in &struct_val.fields {
            // Convert prost_types::Value to CEL Value
            let cel_value = convert_protobuf_value_to_cel(value)?;
            map_entries.insert(cel::objects::Key::String(Arc::new(key.clone())), cel_value);
        }
        fields.insert(
            "single_struct".to_string(),
            cel::objects::Value::Map(cel::objects::Map {
                map: Arc::new(map_entries),
            }),
        );
    }

    if let Some(ref timestamp) = msg.single_timestamp {
        // Convert google.protobuf.Timestamp to CEL Timestamp
        use chrono::{DateTime, TimeZone, Utc};
        let ts = Utc
            .timestamp_opt(timestamp.seconds, timestamp.nanos as u32)
            .single()
            .ok_or_else(|| ConversionError::Unsupported("Invalid timestamp".to_string()))?;
        let fixed_offset = DateTime::from_naive_utc_and_offset(
            ts.naive_utc(),
            chrono::FixedOffset::east_opt(0).unwrap(),
        );
        fields.insert("single_timestamp".to_string(), Timestamp(fixed_offset));
    }

    // Handle single_any field
    if let Some(ref any) = msg.single_any {
        match convert_any_to_cel_value(any) {
            Ok(cel_value) => {
                fields.insert("single_any".to_string(), cel_value);
            }
            Err(_) => {
                fields.insert("single_any".to_string(), CelValue::Null);
            }
        }
    }

    if let Some(ref duration) = msg.single_duration {
        // Convert google.protobuf.Duration to CEL Duration
        use chrono::Duration as ChronoDuration;
        let dur = ChronoDuration::seconds(duration.seconds)
            + ChronoDuration::nanoseconds(duration.nanos as i64);
        fields.insert("single_duration".to_string(), Duration(dur));
    }

    if let Some(ref value) = msg.single_value {
        // Convert google.protobuf.Value to CEL Value
        let cel_value = convert_protobuf_value_to_cel(value)?;
        fields.insert("single_value".to_string(), cel_value);
    }

    if let Some(ref list_value) = msg.list_value {
        // Convert google.protobuf.ListValue to CEL List
        let mut list_items = Vec::new();
        for item in &list_value.values {
            list_items.push(convert_protobuf_value_to_cel(item)?);
        }
        fields.insert("list_value".to_string(), List(Arc::new(list_items)));
    }

    // Handle repeated fields
    if !msg.repeated_int32.is_empty() {
        let values: Vec<CelValue> = msg.repeated_int32.iter().map(|&v| Int(v as i64)).collect();
        fields.insert("repeated_int32".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_int64.is_empty() {
        let values: Vec<CelValue> = msg.repeated_int64.iter().map(|&v| Int(v)).collect();
        fields.insert("repeated_int64".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_uint32.is_empty() {
        let values: Vec<CelValue> = msg
            .repeated_uint32
            .iter()
            .map(|&v| UInt(v as u64))
            .collect();
        fields.insert("repeated_uint32".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_uint64.is_empty() {
        let values: Vec<CelValue> = msg.repeated_uint64.iter().map(|&v| UInt(v)).collect();
        fields.insert("repeated_uint64".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_float.is_empty() {
        let values: Vec<CelValue> = msg
            .repeated_float
            .iter()
            .map(|&v| Float(v as f64))
            .collect();
        fields.insert("repeated_float".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_double.is_empty() {
        let values: Vec<CelValue> = msg.repeated_double.iter().map(|&v| Float(v)).collect();
        fields.insert("repeated_double".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_bool.is_empty() {
        let values: Vec<CelValue> = msg.repeated_bool.iter().map(|&v| Bool(v)).collect();
        fields.insert("repeated_bool".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_string.is_empty() {
        let values: Vec<CelValue> = msg
            .repeated_string
            .iter()
            .map(|v| String(Arc::new(v.clone())))
            .collect();
        fields.insert("repeated_string".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_bytes.is_empty() {
        let values: Vec<CelValue> = msg
            .repeated_bytes
            .iter()
            .map(|v| Bytes(Arc::new(v.to_vec())))
            .collect();
        fields.insert("repeated_bytes".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_sint32.is_empty() {
        let values: Vec<CelValue> = msg.repeated_sint32.iter().map(|&v| Int(v as i64)).collect();
        fields.insert("repeated_sint32".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_sint64.is_empty() {
        let values: Vec<CelValue> = msg.repeated_sint64.iter().map(|&v| Int(v)).collect();
        fields.insert("repeated_sint64".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_fixed32.is_empty() {
        let values: Vec<CelValue> = msg
            .repeated_fixed32
            .iter()
            .map(|&v| UInt(v as u64))
            .collect();
        fields.insert("repeated_fixed32".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_fixed64.is_empty() {
        let values: Vec<CelValue> = msg.repeated_fixed64.iter().map(|&v| UInt(v)).collect();
        fields.insert("repeated_fixed64".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_sfixed32.is_empty() {
        let values: Vec<CelValue> = msg
            .repeated_sfixed32
            .iter()
            .map(|&v| Int(v as i64))
            .collect();
        fields.insert("repeated_sfixed32".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_sfixed64.is_empty() {
        let values: Vec<CelValue> = msg.repeated_sfixed64.iter().map(|&v| Int(v)).collect();
        fields.insert("repeated_sfixed64".to_string(), List(Arc::new(values)));
    }
    if !msg.repeated_nested_enum.is_empty() {
        let values: Vec<CelValue> = msg
            .repeated_nested_enum
            .iter()
            .map(|&v| {
                Enum(
                    v as i64,
                    Arc::new("cel.expr.conformance.proto3.TestAllTypes.NestedEnum".to_string()),
                )
            })
            .collect();
        fields.insert("repeated_nested_enum".to_string(), List(Arc::new(values)));
    }

    // Handle map fields
    if !msg.map_int32_int64.is_empty() {
        let mut map_entries = HashMap::new();
        for (&k, &v) in &msg.map_int32_int64 {
            map_entries.insert(cel::objects::Key::Int(k as i64), Int(v));
        }
        fields.insert(
            "map_int32_int64".to_string(),
            cel::objects::Value::Map(cel::objects::Map {
                map: Arc::new(map_entries),
            }),
        );
    }
    if !msg.map_string_string.is_empty() {
        let mut map_entries = HashMap::new();
        for (k, v) in &msg.map_string_string {
            map_entries.insert(
                cel::objects::Key::String(Arc::new(k.clone())),
                String(Arc::new(v.clone())),
            );
        }
        fields.insert(
            "map_string_string".to_string(),
            cel::objects::Value::Map(cel::objects::Map {
                map: Arc::new(map_entries),
            }),
        );
    }
    if !msg.map_int64_int64.is_empty() {
        let mut map_entries = HashMap::new();
        for (&k, &v) in &msg.map_int64_int64 {
            map_entries.insert(cel::objects::Key::Int(k), Int(v));
        }
        fields.insert(
            "map_int64_int64".to_string(),
            cel::objects::Value::Map(cel::objects::Map {
                map: Arc::new(map_entries),
            }),
        );
    }
    if !msg.map_uint64_uint64.is_empty() {
        let mut map_entries = HashMap::new();
        for (&k, &v) in &msg.map_uint64_uint64 {
            map_entries.insert(cel::objects::Key::Uint(k), UInt(v));
        }
        fields.insert(
            "map_uint64_uint64".to_string(),
            cel::objects::Value::Map(cel::objects::Map {
                map: Arc::new(map_entries),
            }),
        );
    }
    if !msg.map_string_int64.is_empty() {
        let mut map_entries = HashMap::new();
        for (k, &v) in &msg.map_string_int64 {
            map_entries.insert(cel::objects::Key::String(Arc::new(k.clone())), Int(v));
        }
        fields.insert(
            "map_string_int64".to_string(),
            cel::objects::Value::Map(cel::objects::Map {
                map: Arc::new(map_entries),
            }),
        );
    }
    if !msg.map_int32_string.is_empty() {
        let mut map_entries = HashMap::new();
        for (&k, v) in &msg.map_int32_string {
            map_entries.insert(
                cel::objects::Key::Int(k as i64),
                String(Arc::new(v.clone())),
            );
        }
        fields.insert(
            "map_int32_string".to_string(),
            cel::objects::Value::Map(cel::objects::Map {
                map: Arc::new(map_entries),
            }),
        );
    }
    if !msg.map_bool_bool.is_empty() {
        let mut map_entries = HashMap::new();
        for (&k, &v) in &msg.map_bool_bool {
            map_entries.insert(cel::objects::Key::Bool(k), Bool(v));
        }
        fields.insert(
            "map_bool_bool".to_string(),
            cel::objects::Value::Map(cel::objects::Map {
                map: Arc::new(map_entries),
            }),
        );
    }

    // If oneof field wasn't set by prost decoding, try to parse it manually from wire format
    // This handles cases where prost-reflect encoding loses oneof information
    if msg.kind.is_none() {
        if let Some((field_name, oneof_value)) = parse_oneof_from_wire_format(original_bytes)? {
            fields.insert(field_name, oneof_value);
        }
    }

    // Filter out reserved keyword fields (fields 500-516) that were formerly CEL reserved identifiers
    // These should not be exposed in the CEL representation
    let reserved_keywords = [
        "as",
        "break",
        "const",
        "continue",
        "else",
        "for",
        "function",
        "if",
        "import",
        "let",
        "loop",
        "package",
        "namespace",
        "return",
        "var",
        "void",
        "while",
    ];
    for keyword in &reserved_keywords {
        fields.remove(*keyword);
    }

    Ok(Struct(Struct {
        type_name: Arc::new("cel.expr.conformance.proto3.TestAllTypes".to_string()),
        fields: Arc::new(fields),
    }))
}

/// Convert a proto2 TestAllTypes message to a CEL Struct
fn convert_test_all_types_proto2_to_struct(
    msg: &crate::proto::cel::expr::conformance::proto2::TestAllTypes,
    original_bytes: &[u8],
) -> Result<CelValue, ConversionError> {
    use cel::objects::{Struct, Value::*};
    use std::sync::Arc;

    let mut fields = HashMap::new();

    // Proto2 has optional fields, so we need to check if they're set
    // Wrapper types are already decoded by prost - convert them to CEL values or Null
    // Unset wrapper fields should map to Null, not be missing from the struct
    fields.insert(
        "single_bool_wrapper".to_string(),
        msg.single_bool_wrapper.map(Bool).unwrap_or(Null),
    );
    fields.insert(
        "single_bytes_wrapper".to_string(),
        msg.single_bytes_wrapper
            .as_ref()
            .map(|v| Bytes(Arc::new(v.clone())))
            .unwrap_or(Null),
    );
    fields.insert(
        "single_double_wrapper".to_string(),
        msg.single_double_wrapper.map(Float).unwrap_or(Null),
    );
    fields.insert(
        "single_float_wrapper".to_string(),
        msg.single_float_wrapper
            .map(|v| Float(v as f64))
            .unwrap_or(Null),
    );
    fields.insert(
        "single_int32_wrapper".to_string(),
        msg.single_int32_wrapper
            .map(|v| Int(v as i64))
            .unwrap_or(Null),
    );
    fields.insert(
        "single_int64_wrapper".to_string(),
        msg.single_int64_wrapper.map(Int).unwrap_or(Null),
    );
    fields.insert(
        "single_string_wrapper".to_string(),
        msg.single_string_wrapper
            .as_ref()
            .map(|v| String(Arc::new(v.clone())))
            .unwrap_or(Null),
    );
    fields.insert(
        "single_uint32_wrapper".to_string(),
        msg.single_uint32_wrapper
            .map(|v| UInt(v as u64))
            .unwrap_or(Null),
    );
    fields.insert(
        "single_uint64_wrapper".to_string(),
        msg.single_uint64_wrapper.map(UInt).unwrap_or(Null),
    );

    // Add other fields (proto2 has defaults)
    fields.insert(
        "single_bool".to_string(),
        Bool(msg.single_bool.unwrap_or(true)),
    );
    if let Some(ref s) = msg.single_string {
        fields.insert("single_string".to_string(), String(Arc::new(s.clone())));
    }
    if let Some(ref b) = msg.single_bytes {
        fields.insert(
            "single_bytes".to_string(),
            Bytes(Arc::new(b.clone().into())),
        );
    }
    if let Some(i) = msg.single_int32 {
        fields.insert("single_int32".to_string(), Int(i as i64));
    }
    if let Some(i) = msg.single_int64 {
        fields.insert("single_int64".to_string(), Int(i));
    }
    if let Some(u) = msg.single_uint32 {
        fields.insert("single_uint32".to_string(), UInt(u as u64));
    }
    if let Some(u) = msg.single_uint64 {
        fields.insert("single_uint64".to_string(), UInt(u));
    }
    if let Some(f) = msg.single_float {
        fields.insert("single_float".to_string(), Float(f as f64));
    }
    if let Some(d) = msg.single_double {
        fields.insert("single_double".to_string(), Float(d));
    }

    // Handle specialized integer types (proto2 optional fields)
    if let Some(i) = msg.single_sint32 {
        fields.insert("single_sint32".to_string(), Int(i as i64));
    }
    if let Some(i) = msg.single_sint64 {
        fields.insert("single_sint64".to_string(), Int(i));
    }
    if let Some(u) = msg.single_fixed32 {
        fields.insert("single_fixed32".to_string(), UInt(u as u64));
    }
    if let Some(u) = msg.single_fixed64 {
        fields.insert("single_fixed64".to_string(), UInt(u));
    }
    if let Some(i) = msg.single_sfixed32 {
        fields.insert("single_sfixed32".to_string(), Int(i as i64));
    }
    if let Some(i) = msg.single_sfixed64 {
        fields.insert("single_sfixed64".to_string(), Int(i));
    }

    // Handle standalone_enum field
    if let Some(e) = msg.standalone_enum {
        fields.insert(
            "standalone_enum".to_string(),
            Enum(
                e as i64,
                Arc::new("cel.expr.conformance.proto2.TestAllTypes.NestedEnum".to_string()),
            ),
        );
    }

    // Handle oneof field (kind) - proto2 version
    if let Some(ref kind) = msg.kind {
        use crate::proto::cel::expr::conformance::proto2::test_all_types::Kind;
        match kind {
            Kind::OneofType(nested) => {
                // Convert NestedTestAllTypes - has child and payload fields
                let mut nested_fields = HashMap::new();

                // Handle child field (recursive NestedTestAllTypes)
                if let Some(ref child) = nested.child {
                    // Recursively convert child (simplified for now - just handle payload)
                    let mut child_fields = HashMap::new();
                    if let Some(ref payload) = child.payload {
                        let payload_struct = convert_test_all_types_proto2_to_struct(payload, &[])?;
                        child_fields.insert("payload".to_string(), payload_struct);
                    }
                    let child_struct = Struct(Struct {
                        type_name: Arc::new(
                            "cel.expr.conformance.proto2.NestedTestAllTypes".to_string(),
                        ),
                        fields: Arc::new(child_fields),
                    });
                    nested_fields.insert("child".to_string(), child_struct);
                }

                // Handle payload field (TestAllTypes)
                if let Some(ref payload) = nested.payload {
                    let payload_struct = convert_test_all_types_proto2_to_struct(payload, &[])?;
                    nested_fields.insert("payload".to_string(), payload_struct);
                }

                let nested_struct = Struct(Struct {
                    type_name: Arc::new(
                        "cel.expr.conformance.proto2.NestedTestAllTypes".to_string(),
                    ),
                    fields: Arc::new(nested_fields),
                });
                fields.insert("oneof_type".to_string(), nested_struct);
            }
            Kind::OneofMsg(nested) => {
                // Convert NestedMessage to struct
                let mut nested_fields = HashMap::new();
                nested_fields.insert("bb".to_string(), Int(nested.bb.unwrap_or(0) as i64));
                let nested_struct = Struct(Struct {
                    type_name: Arc::new("cel.expr.conformance.proto2.NestedMessage".to_string()),
                    fields: Arc::new(nested_fields),
                });
                fields.insert("oneof_msg".to_string(), nested_struct);
            }
            Kind::OneofBool(b) => {
                fields.insert("oneof_bool".to_string(), Bool(*b));
            }
        }
    }

    // Handle optional message fields (well-known types)
    if let Some(ref struct_val) = msg.single_struct {
        // Convert google.protobuf.Struct to CEL Map
        let mut map_entries = HashMap::new();
        for (key, value) in &struct_val.fields {
            let cel_value = convert_protobuf_value_to_cel(value)?;
            map_entries.insert(cel::objects::Key::String(Arc::new(key.clone())), cel_value);
        }
        fields.insert(
            "single_struct".to_string(),
            cel::objects::Value::Map(cel::objects::Map {
                map: Arc::new(map_entries),
            }),
        );
    }

    if let Some(ref timestamp) = msg.single_timestamp {
        // Convert google.protobuf.Timestamp to CEL Timestamp
        use chrono::{DateTime, TimeZone, Utc};
        let ts = Utc
            .timestamp_opt(timestamp.seconds, timestamp.nanos as u32)
            .single()
            .ok_or_else(|| ConversionError::Unsupported("Invalid timestamp".to_string()))?;
        let fixed_offset = DateTime::from_naive_utc_and_offset(
            ts.naive_utc(),
            chrono::FixedOffset::east_opt(0).unwrap(),
        );
        fields.insert("single_timestamp".to_string(), Timestamp(fixed_offset));
    }

    // Handle single_any field
    if let Some(ref any) = msg.single_any {
        match convert_any_to_cel_value(any) {
            Ok(cel_value) => {
                fields.insert("single_any".to_string(), cel_value);
            }
            Err(_) => {
                fields.insert("single_any".to_string(), CelValue::Null);
            }
        }
    }

    if let Some(ref duration) = msg.single_duration {
        // Convert google.protobuf.Duration to CEL Duration
        use chrono::Duration as ChronoDuration;
        let dur = ChronoDuration::seconds(duration.seconds)
            + ChronoDuration::nanoseconds(duration.nanos as i64);
        fields.insert("single_duration".to_string(), Duration(dur));
    }

    if let Some(ref value) = msg.single_value {
        // Convert google.protobuf.Value to CEL Value
        let cel_value = convert_protobuf_value_to_cel(value)?;
        fields.insert("single_value".to_string(), cel_value);
    }

    if let Some(ref list_value) = msg.list_value {
        // Convert google.protobuf.ListValue to CEL List
        let mut list_items = Vec::new();
        for item in &list_value.values {
            list_items.push(convert_protobuf_value_to_cel(item)?);
        }
        fields.insert("list_value".to_string(), List(Arc::new(list_items)));
    }

    // Handle repeated enum fields
    if !msg.repeated_nested_enum.is_empty() {
        let values: Vec<CelValue> = msg
            .repeated_nested_enum
            .iter()
            .map(|&v| {
                Enum(
                    v as i64,
                    Arc::new("cel.expr.conformance.proto2.TestAllTypes.NestedEnum".to_string()),
                )
            })
            .collect();
        fields.insert("repeated_nested_enum".to_string(), List(Arc::new(values)));
    }

    // Before returning the struct, extract extension fields from wire format
    extract_extension_fields(original_bytes, &mut fields)?;

    // Filter out reserved keyword fields (fields 500-516) that were formerly CEL reserved identifiers
    // These should not be exposed in the CEL representation
    let reserved_keywords = [
        "as",
        "break",
        "const",
        "continue",
        "else",
        "for",
        "function",
        "if",
        "import",
        "let",
        "loop",
        "package",
        "namespace",
        "return",
        "var",
        "void",
        "while",
    ];
    for keyword in &reserved_keywords {
        fields.remove(*keyword);
    }

    Ok(Struct(Struct {
        type_name: Arc::new("cel.expr.conformance.proto2.TestAllTypes".to_string()),
        fields: Arc::new(fields),
    }))
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

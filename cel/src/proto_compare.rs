//! Protobuf wire format parser for semantic comparison of Any values.
//!
//! This module implements a generic protobuf wire format parser that can compare
//! two serialized protobuf messages semantically, even if they have different
//! field orders. This is used to compare `google.protobuf.Any` values correctly.

use std::collections::HashMap;

/// A parsed protobuf field value
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// Variable-length integer (wire type 0)
    Varint(u64),
    /// 64-bit value (wire type 1)
    Fixed64([u8; 8]),
    /// Length-delimited value (wire type 2) - strings, bytes, messages
    LengthDelimited(Vec<u8>),
    /// 32-bit value (wire type 5)
    Fixed32([u8; 4]),
}

/// Map from field number to list of values (fields can appear multiple times)
type FieldMap = HashMap<u32, Vec<FieldValue>>;

/// Decode a varint from the beginning of a byte slice.
/// Returns the decoded value and the number of bytes consumed.
fn decode_varint(bytes: &[u8]) -> Option<(u64, usize)> {
    let mut result = 0u64;
    let mut shift = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        if shift >= 64 {
            return None; // Overflow
        }
        result |= ((byte & 0x7F) as u64) << shift;
        if (byte & 0x80) == 0 {
            return Some((result, i + 1));
        }
        shift += 7;
    }
    None // Incomplete varint
}

/// Parse protobuf wire format into a field map.
/// Returns None if the bytes cannot be parsed as valid protobuf.
pub fn parse_proto_wire_format(bytes: &[u8]) -> Option<FieldMap> {
    let mut field_map: FieldMap = HashMap::new();
    let mut pos = 0;

    while pos < bytes.len() {
        // Read field tag (field_number << 3 | wire_type)
        let (tag, tag_len) = decode_varint(&bytes[pos..])?;
        pos += tag_len;

        let field_number = (tag >> 3) as u32;
        let wire_type = (tag & 0x07) as u8;

        // Parse field value based on wire type
        let field_value = match wire_type {
            0 => {
                // Varint
                let (value, len) = decode_varint(&bytes[pos..])?;
                pos += len;
                FieldValue::Varint(value)
            }
            1 => {
                // Fixed64
                if pos + 8 > bytes.len() {
                    return None;
                }
                let mut buf = [0u8; 8];
                buf.copy_from_slice(&bytes[pos..pos + 8]);
                pos += 8;
                FieldValue::Fixed64(buf)
            }
            2 => {
                // Length-delimited
                let (len, len_bytes) = decode_varint(&bytes[pos..])?;
                pos += len_bytes;
                let len = len as usize;
                if pos + len > bytes.len() {
                    return None;
                }
                let value = bytes[pos..pos + len].to_vec();
                pos += len;
                FieldValue::LengthDelimited(value)
            }
            5 => {
                // Fixed32
                if pos + 4 > bytes.len() {
                    return None;
                }
                let mut buf = [0u8; 4];
                buf.copy_from_slice(&bytes[pos..pos + 4]);
                pos += 4;
                FieldValue::Fixed32(buf)
            }
            _ => {
                // Unknown wire type, cannot parse
                return None;
            }
        };

        // Add field to map (fields can appear multiple times)
        field_map.entry(field_number).or_default().push(field_value);
    }

    Some(field_map)
}

/// Compare two field values semantically.
///
/// `depth` parameter controls recursion depth. We only recursively parse
/// nested messages at depth 0 (top level). For deeper levels, we use
/// bytewise comparison to avoid infinite recursion and to handle cases
/// where length-delimited fields are strings/bytes rather than nested messages.
fn compare_field_values(a: &FieldValue, b: &FieldValue, depth: usize) -> bool {
    match (a, b) {
        (FieldValue::Varint(a), FieldValue::Varint(b)) => a == b,
        (FieldValue::Fixed64(a), FieldValue::Fixed64(b)) => a == b,
        (FieldValue::Fixed32(a), FieldValue::Fixed32(b)) => a == b,
        (FieldValue::LengthDelimited(a), FieldValue::LengthDelimited(b)) => {
            // Try recursive parsing for nested messages at top level only
            // This allows comparing messages with different field orders
            if depth == 0 {
                // Try to parse as nested protobuf messages and compare semantically
                // If parsing fails, fall back to bytewise comparison
                match (parse_proto_wire_format(a), parse_proto_wire_format(b)) {
                    (Some(map_a), Some(map_b)) => {
                        // Both are valid protobuf messages, compare semantically
                        compare_field_maps_with_depth(&map_a, &map_b, depth + 1)
                    }
                    _ => {
                        // Either not valid protobuf or parsing failed
                        // Fall back to bytewise comparison (for strings, bytes, etc.)
                        a == b
                    }
                }
            } else {
                // At deeper levels, use bytewise comparison
                a == b
            }
        }
        _ => false, // Different types
    }
}

/// Compare two field maps semantically with depth tracking.
fn compare_field_maps_with_depth(a: &FieldMap, b: &FieldMap, depth: usize) -> bool {
    // Check if both have the same field numbers
    if a.len() != b.len() {
        return false;
    }

    // Compare each field
    for (field_num, values_a) in a.iter() {
        match b.get(field_num) {
            Some(values_b) => {
                // Check if both have same number of values
                if values_a.len() != values_b.len() {
                    return false;
                }
                // Compare each value
                for (val_a, val_b) in values_a.iter().zip(values_b.iter()) {
                    if !compare_field_values(val_a, val_b, depth) {
                        return false;
                    }
                }
            }
            None => return false, // Field missing in b
        }
    }

    true
}

/// Compare two field maps semantically (top-level entry point).
fn compare_field_maps(a: &FieldMap, b: &FieldMap) -> bool {
    compare_field_maps_with_depth(a, b, 0)
}

/// Convert a FieldValue to a CEL Value.
/// This is a best-effort conversion for unpacking Any values.
pub fn field_value_to_cel(field_value: &FieldValue) -> crate::objects::Value {
    use crate::objects::Value;
    use std::sync::Arc;

    match field_value {
        FieldValue::Varint(v) => {
            // Varint could be int, uint, bool, or enum
            // For simplicity, treat as Int if it fits in i64, otherwise UInt
            if *v <= i64::MAX as u64 {
                Value::Int(*v as i64)
            } else {
                Value::UInt(*v)
            }
        }
        FieldValue::Fixed64(bytes) => {
            // Could be fixed64, sfixed64, or double
            // Try to interpret as double (most common for field 12 in TestAllTypes)
            let value = f64::from_le_bytes(*bytes);
            Value::Float(value)
        }
        FieldValue::Fixed32(bytes) => {
            // Could be fixed32, sfixed32, or float
            // Try to interpret as float (most common)
            let value = f32::from_le_bytes(*bytes);
            Value::Float(value as f64)
        }
        FieldValue::LengthDelimited(bytes) => {
            // Could be string, bytes, or nested message
            // Try to decode as UTF-8 string first
            if let Ok(s) = std::str::from_utf8(bytes) {
                Value::String(Arc::new(s.to_string()))
            } else {
                // Not valid UTF-8, treat as bytes
                Value::Bytes(Arc::new(bytes.clone()))
            }
        }
    }
}

/// Compare two protobuf wire-format byte arrays semantically.
///
/// This function parses both byte arrays as protobuf wire format and compares
/// the resulting field maps. Two messages are considered equal if they have the
/// same fields with the same values, regardless of field order.
///
/// If either byte array cannot be parsed as valid protobuf, falls back to
/// bytewise comparison.
pub fn compare_any_values_semantic(value_a: &[u8], value_b: &[u8]) -> bool {
    // Try to parse both as protobuf wire format
    match (
        parse_proto_wire_format(value_a),
        parse_proto_wire_format(value_b),
    ) {
        (Some(map_a), Some(map_b)) => {
            // Compare the parsed field maps semantically
            compare_field_maps(&map_a, &map_b)
        }
        _ => {
            // If either cannot be parsed, fall back to bytewise comparison
            value_a == value_b
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_varint() {
        // Test simple values
        assert_eq!(decode_varint(&[0x00]), Some((0, 1)));
        assert_eq!(decode_varint(&[0x01]), Some((1, 1)));
        assert_eq!(decode_varint(&[0x7F]), Some((127, 1)));

        // Test multi-byte varint
        assert_eq!(decode_varint(&[0x80, 0x01]), Some((128, 2)));
        assert_eq!(decode_varint(&[0xAC, 0x02]), Some((300, 2)));

        // Test incomplete varint
        assert_eq!(decode_varint(&[0x80]), None);
    }

    #[test]
    fn test_parse_simple_message() {
        // Message with field 1 (varint) = 150
        let bytes = vec![0x08, 0x96, 0x01];
        let map = parse_proto_wire_format(&bytes).unwrap();

        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&1).unwrap().len(), 1);
        assert_eq!(map.get(&1).unwrap()[0], FieldValue::Varint(150));
    }

    #[test]
    fn test_compare_different_field_order() {
        // Message 1: field 1 = 1234, field 2 = "test"
        let bytes_a = vec![
            0x08, 0xD2, 0x09, // field 1, varint 1234
            0x12, 0x04, 0x74, 0x65, 0x73, 0x74, // field 2, string "test"
        ];

        // Message 2: field 2 = "test", field 1 = 1234 (different order)
        let bytes_b = vec![
            0x12, 0x04, 0x74, 0x65, 0x73, 0x74, // field 2, string "test"
            0x08, 0xD2, 0x09, // field 1, varint 1234
        ];

        assert!(compare_any_values_semantic(&bytes_a, &bytes_b));
    }

    #[test]
    fn test_compare_different_values() {
        // Message 1: field 1 = 1234
        let bytes_a = vec![0x08, 0xD2, 0x09];

        // Message 2: field 1 = 5678
        let bytes_b = vec![0x08, 0xAE, 0x2C];

        assert!(!compare_any_values_semantic(&bytes_a, &bytes_b));
    }

    #[test]
    fn test_fallback_to_bytewise() {
        // Invalid protobuf (incomplete varint)
        let bytes_a = vec![0x08, 0x80];
        let bytes_b = vec![0x08, 0x80];

        // Should fall back to bytewise comparison
        assert!(compare_any_values_semantic(&bytes_a, &bytes_b));

        let bytes_c = vec![0x08, 0x81];
        assert!(!compare_any_values_semantic(&bytes_a, &bytes_c));
    }
}

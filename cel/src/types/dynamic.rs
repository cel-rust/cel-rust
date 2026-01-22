use crate::Value;
use cel::{to_value, types};

pub fn maybe_materialize_optional<T: DynamicType>(t: &Option<T>) -> Value<'_> {
    match t {
        Some(v) => maybe_materialize(v),
        None => Value::Null,
    }
}
pub fn maybe_materialize<T: DynamicType>(t: &T) -> Value<'_> {
    if t.auto_materialize() {
        t.materialize()
    } else {
        Value::Dynamic(DynamicValue::new(t))
    }
}

pub fn always_materialize(t: Value) -> Value {
    if let Value::Dynamic(d) = t {
        d.materialize()
    } else {
        t
    }
}

pub trait DynamicType: std::fmt::Debug + Send + Sync {
    // If the value can be freely converted to a Value, do so.
    // This is anything but list/map
    fn auto_materialize(&self) -> bool {
        false
    }

    // Convert this dynamic value into a proper value
    fn materialize(&self) -> Value<'_>;

    fn field(&self, _field: &str) -> Option<Value<'_>> {
        None
    }
}

pub struct DynamicValue<'a> {
    dyn_ref: &'a dyn DynamicType,
}

impl<'a> DynamicValue<'a> {
    pub fn new<T: DynamicType>(t: &'a T) -> Self {
        Self {
            dyn_ref: t as &dyn DynamicType,
        }
    }

    pub fn materialize(&self) -> Value<'a> {
        self.dyn_ref.materialize()
    }

    pub fn field(&self, field: &str) -> Option<Value<'a>> {
        self.dyn_ref.field(field)
    }
}

impl<'a> Clone for DynamicValue<'a> {
    fn clone(&self) -> Self {
        Self {
            dyn_ref: self.dyn_ref,
        }
    }
}

impl<'a> std::fmt::Debug for DynamicValue<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.dyn_ref.fmt(f)
    }
}

impl DynamicType for Value<'_> {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        self.clone()
    }
}

// Primitive type implementations

// &str - auto-materializes to String value
impl DynamicType for &str {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(*self)
    }
}

// String - auto-materializes to String value
impl DynamicType for String {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(self.as_str())
    }
}

// bool - auto-materializes to Bool value
impl DynamicType for bool {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(*self)
    }
}

// i64 - auto-materializes to Int value
impl DynamicType for i64 {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(*self)
    }
}

// u64 - auto-materializes to Int value (as i64)
impl DynamicType for u64 {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(*self as i64)
    }
}

// i32 - auto-materializes to Int value
impl DynamicType for i32 {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(*self as i64)
    }
}

// u32 - auto-materializes to Int value
impl DynamicType for u32 {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(*self as u64)
    }
}

// f64 - auto-materializes to Float value
impl DynamicType for f64 {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(*self)
    }
}

// Collection types - these do NOT auto-materialize since they're complex structures

// HashMap<String, String> - materializes to Map value
impl DynamicType for std::collections::HashMap<String, String> {
    fn auto_materialize(&self) -> bool {
        false // Maps are complex, don't auto-materialize
    }

    fn materialize(&self) -> Value<'_> {
        let mut map = vector_map::VecMap::with_capacity(self.len());
        for (k, v) in self.iter() {
            map.insert(
                crate::objects::KeyRef::from(k.as_str()),
                Value::from(v.as_str()),
            );
        }
        Value::Map(crate::objects::MapValue::Borrow(map))
    }

    fn field(&self, field: &str) -> Option<Value<'_>> {
        self.get(field).map(|v| Value::from(v.as_str()))
    }
}

// &HashMap<String, String> - reference to HashMap
impl DynamicType for &std::collections::HashMap<String, String> {
    fn auto_materialize(&self) -> bool {
        false
    }

    fn materialize(&self) -> Value<'_> {
        let mut map = vector_map::VecMap::with_capacity(self.len());
        for (k, v) in self.iter() {
            map.insert(
                crate::objects::KeyRef::from(k.as_str()),
                Value::from(v.as_str()),
            );
        }
        Value::Map(crate::objects::MapValue::Borrow(map))
    }

    fn field(&self, field: &str) -> Option<Value<'_>> {
        self.get(field).map(|v| Value::from(v.as_str()))
    }
}

impl DynamicType for &http::HeaderMap {
    fn auto_materialize(&self) -> bool {
        false // Maps are complex, don't auto-materialize
    }

    fn materialize(&self) -> Value<'_> {
        let mut map = vector_map::VecMap::with_capacity(self.len());
        for (k, v) in self.iter() {
            if let Ok(s) = str::from_utf8(v.as_bytes()) {
                map.insert(crate::objects::KeyRef::from(k.as_str()), Value::from(s));
            }
        }
        Value::Map(crate::objects::MapValue::Borrow(map))
    }

    fn field(&self, field: &str) -> Option<Value<'_>> {
        // TODO: do not implicitly drop invalid utf8
        self.get(field)
            .and_then(|v| Some(Value::from(str::from_utf8(v.as_bytes()).ok()?)))
    }
}

// Vec<String> - materializes to List value
impl DynamicType for Vec<String> {
    fn auto_materialize(&self) -> bool {
        false // Lists are complex, don't auto-materialize
    }

    fn materialize(&self) -> Value<'_> {
        let items: Vec<Value<'static>> = self.iter().map(|s| Value::from(s.clone())).collect();
        Value::List(crate::objects::ListValue::Owned(items.into()))
    }
}

// &[String] - slice of Strings
impl DynamicType for &[String] {
    fn auto_materialize(&self) -> bool {
        false
    }

    fn materialize(&self) -> Value<'_> {
        let items: Vec<Value<'static>> = self.iter().map(|s| Value::from(s.clone())).collect();
        Value::List(crate::objects::ListValue::Owned(items.into()))
    }
}

impl<'a> DynamicType for serde_json::Value {
    fn materialize(&self) -> Value<'_> {
        to_value(self).unwrap()
    }
    fn auto_materialize(&self) -> bool {
        false
    }

    fn field(&self, field: &str) -> Option<Value<'_>> {
        match self {
            serde_json::Value::Object(m) => {
                let v = m.get(field)?;
                Some(types::dynamic::maybe_materialize(v))
            }
            _ => None,
        }
    }
}
impl<'a> DynamicType for serde_json::Map<String, serde_json::Value> {
    fn materialize(&self) -> Value<'_> {
        to_value(self).unwrap()
    }
    fn auto_materialize(&self) -> bool {
        false
    }

    fn field(&self, field: &str) -> Option<Value<'_>> {
        let v = self.get(field)?;
        Some(types::dynamic::maybe_materialize(v))
    }
}

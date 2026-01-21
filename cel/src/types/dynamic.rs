use serde::Serialize;
use crate::Value;

pub trait DynamicValue<'a>: std::fmt::Debug + Send + Sync + erased_serde::Serialize {
    // If the value can be freely converted to a Value, do so.
    // This is anything but list/map
    fn maybe_materialize(&'a self) -> Option<Value<'a>> {
        None
    }

    // Convert this dynamic value into a proper value
    fn materialize(&'a self) -> Value<'_>;

    fn field(&'a self, field: &str) -> Option<Value<'a>>;
}

use crate::objects::Opaque;
use crate::{ExecutionError, Value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionalValue {
    value: Option<Value<'static>>,
}

impl OptionalValue {
    pub fn of(value: Value<'static>) -> Self {
        OptionalValue { value: Some(value) }
    }
    pub fn none() -> Self {
        OptionalValue { value: None }
    }
    pub fn value(&self) -> Option<&Value<'static>> {
        self.value.as_ref()
    }
}

impl Opaque for OptionalValue {
    fn type_name(&self) -> &'static str {
        "optional_type"
    }
    fn json(&self) -> Option<serde_json::Value> {
        match self.value.as_ref() {
            // TODO: or exclude it?
            None => Some(serde_json::Value::Null),
            Some(v) => v.json().ok(),
        }
    }
}

impl<'a> TryFrom<Value<'a>> for OptionalValue {
    type Error = ExecutionError;

    fn try_from(value: Value<'a>) -> Result<Self, Self::Error> {
        match value {
            Value::Object(obj) if obj.type_name() == "optional_type" => obj
                .downcast_ref::<OptionalValue>()
                .ok_or_else(|| ExecutionError::function_error("optional", "failed to downcast"))
                .cloned(),
            Value::Object(obj) => Err(ExecutionError::UnexpectedType {
                got: obj.type_name(),
                want: "optional_type",
            }),
            v => Err(ExecutionError::UnexpectedType {
                got: v.type_of().as_str(),
                want: "optional_type",
            }),
        }
    }
}

impl<'a, 'b: 'a> TryFrom<&'b Value<'a>> for &'b OptionalValue {
    type Error = ExecutionError;

    fn try_from(value: &'b Value<'a>) -> Result<Self, Self::Error> {
        match value {
            Value::Object(obj) if obj.type_name() == "optional_type" => obj
                .downcast_ref::<OptionalValue>()
                .ok_or_else(|| ExecutionError::function_error("optional", "failed to downcast")),
            Value::Object(obj) => Err(ExecutionError::UnexpectedType {
                got: obj.type_name(),
                want: "optional_type",
            }),
            v => Err(ExecutionError::UnexpectedType {
                got: v.type_of().as_str(),
                want: "optional_type",
            }),
        }
    }
}

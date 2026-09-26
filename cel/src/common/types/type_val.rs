use crate::common::types::{Kind, Type};
use crate::common::value::{CowVal, StaticVal, Val};
use crate::ExecutionError;
use std::any::Any;
use std::borrow::Cow;

static TYPE_TYPE: Type = Type::simple_type(Kind::Type, "type");

#[derive(Clone, Debug)]
pub struct CelType {
    name: Cow<'static, str>,
}

impl CelType {
    pub const fn new_static(name: &'static str) -> Self {
        Self {
            name: Cow::Borrowed(name),
        }
    }

    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: Cow::Owned(name.into()),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// The type value of `t`, sharing its name when it is `'static`.
impl From<&Type> for CelType {
    fn from(t: &Type) -> Self {
        Self {
            name: t.runtime_type_name.clone(),
        }
    }
}

impl Val for CelType {
    fn get_type(&self) -> &Type {
        &TYPE_TYPE
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other
            .downcast_ref::<CelType>()
            .is_some_and(|o| self.name == o.name)
    }

    fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v>
    where
        Self: 'v,
    {
        Box::new(self.clone())
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

impl StaticVal for CelType {}

fn type_of<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let name = args[0].as_ref().get_type().name().to_string();
    Ok(CowVal::owned(CelType::new(name)))
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    env.add_type(crate::common::types::TYPE_TYPE)
        .expect("Must be unique");
    env.add_overload(
        "type",
        "type_of",
        vec![crate::common::types::DYN_TYPE],
        type_of,
    )
    .expect("Must be unique");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::{CelBool, CelInt};

    #[test]
    fn equals_by_name() {
        let a = CelType::new("bool");
        let b = CelType::new("bool");
        let c = CelType::new("int");
        assert!(a.equals(&b));
        assert!(!a.equals(&c));
    }

    #[test]
    fn not_equal_to_other_val_types() {
        let t = CelType::new("bool");
        let b = CelBool::from(true);
        assert!(!t.equals(&b));
    }

    #[test]
    fn type_of_returns_named_type() {
        let args: Vec<CowVal<'_, '_>> = vec![CowVal::owned(CelInt::from(1))];
        let out = type_of(args).unwrap();
        let t = out.as_ref().downcast_ref::<CelType>().unwrap();
        assert_eq!(t.name(), "int");
    }

    #[test]
    fn from_type_shares_a_static_name() {
        let t = CelType::from(&crate::common::types::INT_TYPE);
        assert!(matches!(t.name, Cow::Borrowed("int")));
        assert!(t.equals(&CelType::new("int")));
    }
}

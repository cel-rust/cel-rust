use crate::common::types::{Kind, Type};
use crate::common::value::Val;
use crate::ExecutionError;
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

    /// Resolves a bare identifier such as `bool` or `null_type` to its
    /// corresponding type value, per the cel-spec `type_denotation` set.
    pub fn for_ident(name: &str) -> Option<CelType> {
        matches!(
            name,
            "bool"
                | "bytes"
                | "double"
                | "int"
                | "list"
                | "map"
                | "null_type"
                | "string"
                | "type"
                | "uint"
        )
        .then(|| CelType::new(name))
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

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(self.clone())
    }
}

fn type_of<'a>(args: Vec<Cow<'a, dyn Val>>) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    let name = args[0].as_ref().get_type().name().to_string();
    Ok(Cow::<dyn Val>::Owned(Box::new(CelType::new(name))))
}

pub(crate) fn stdlib(env: &mut crate::Env) {
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
        let args: Vec<Cow<dyn Val>> = vec![Cow::<dyn Val>::Owned(Box::new(CelInt::from(1)))];
        let out = type_of(args).unwrap();
        let t = out.as_ref().downcast_ref::<CelType>().unwrap();
        assert_eq!(t.name(), "int");
    }

    #[test]
    fn for_ident_recognizes_known_names() {
        assert!(CelType::for_ident("bool").is_some());
        assert!(CelType::for_ident("null_type").is_some());
        assert!(CelType::for_ident("type").is_some());
        assert!(CelType::for_ident("something_else").is_none());
    }
}

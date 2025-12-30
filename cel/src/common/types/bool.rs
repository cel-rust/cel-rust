use crate::common::types::Type;
use crate::common::value::Val;
use std::ops::Deref;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct Bool(bool);

impl Bool {
    pub fn negate(&self) -> Self {
        Self(!self.0)
    }

    pub fn into_inner(self) -> bool {
        self.0
    }

    pub fn inner(&self) -> &bool {
        &self.0
    }
}

impl Deref for Bool {
    type Target = bool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Val for Bool {
    fn get_type(&self) -> Type<'_> {
        super::BOOL_TYPE
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other.downcast_ref::<Self>().is_some_and(|a| self.0 == a.0)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(*self)
    }
}

impl From<Bool> for bool {
    fn from(value: Bool) -> Self {
        value.0
    }
}

impl From<bool> for Bool {
    fn from(value: bool) -> Self {
        Bool(value)
    }
}

impl TryFrom<Box<dyn Val>> for bool {
    type Error = Box<dyn Val>;

    fn try_from(value: Box<dyn Val>) -> Result<Self, Self::Error> {
        if let Some(b) = value.downcast_ref::<Bool>() {
            return Ok(b.0);
        }
        Err(value)
    }
}

impl<'a> TryFrom<&'a dyn Val> for &'a bool {
    type Error = &'a dyn Val;

    fn try_from(value: &'a dyn Val) -> Result<Self, Self::Error> {
        if let Some(b) = value.downcast_ref::<Bool>() {
            return Ok(&b.0);
        }
        Err(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types;
    use crate::common::types::Kind;

    #[test]
    fn test_from() {
        let value: Bool = true.into();
        let v: bool = value.into();
        assert!(v)
    }

    #[test]
    fn test_type() {
        let value = Bool(true);
        assert_eq!(value.get_type(), types::BOOL_TYPE);
        assert_eq!(value.get_type().kind, Kind::Boolean);
    }
}

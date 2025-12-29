use crate::common::types::Type;
use crate::common::value::Val;
use std::ops::Deref;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bytes(Vec<u8>);

impl Bytes {
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }

    pub fn inner(&self) -> &[u8] {
        &self.0
    }
}

impl Deref for Bytes {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

impl Val for Bytes {
    fn get_type(&self) -> Type<'_> {
        super::BYTES_TYPE
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other
            .downcast_ref::<Self>()
            .is_some_and(|a| self.0.eq(&a.0))
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Bytes(self.0.clone()))
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(value: Vec<u8>) -> Self {
        Bytes(value)
    }
}

impl From<Bytes> for Vec<u8> {
    fn from(value: Bytes) -> Self {
        value.0
    }
}

impl TryFrom<Box<dyn Val>> for Vec<u8> {
    type Error = Box<dyn Val>;

    fn try_from(value: Box<dyn Val>) -> Result<Self, Self::Error> {
        super::cast_boxed::<Bytes>(value).map(|b| b.into_inner())
    }
}

impl<'a> TryFrom<&'a dyn Val> for &'a [u8] {
    type Error = &'a dyn Val;

    fn try_from(value: &'a dyn Val) -> Result<Self, Self::Error> {
        if let Some(bytes) = value.downcast_ref::<Bytes>() {
            return Ok(bytes.inner());
        }
        Err(value)
    }
}

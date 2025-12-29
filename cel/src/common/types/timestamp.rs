use crate::common::types::Type;
use crate::common::value::Val;
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq)]
pub struct Timestamp(SystemTime);

impl Timestamp {
    pub fn into_inner(self) -> SystemTime {
        self.0
    }

    pub fn inner(&self) -> &SystemTime {
        &self.0
    }
}

impl Val for Timestamp {
    fn get_type(&self) -> Type<'_> {
        super::TIMESTAMP_TYPE
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Timestamp(self.0))
    }
}

impl From<SystemTime> for Timestamp {
    fn from(system_time: SystemTime) -> Self {
        Self(system_time)
    }
}

impl From<Timestamp> for SystemTime {
    fn from(timestamp: Timestamp) -> Self {
        timestamp.0
    }
}

impl TryFrom<Box<dyn Val>> for SystemTime {
    type Error = Box<dyn Val>;

    fn try_from(value: Box<dyn Val>) -> Result<Self, Self::Error> {
        if let Some(ts) = value.downcast_ref::<Timestamp>() {
            return Ok(ts.0);
        }
        Err(value)
    }
}

impl<'a> TryFrom<&'a dyn Val> for &'a SystemTime {
    type Error = &'a dyn Val;

    fn try_from(value: &'a dyn Val) -> Result<Self, Self::Error> {
        if let Some(ts) = value.downcast_ref::<Timestamp>() {
            return Ok(&ts.0);
        }
        Err(value)
    }
}

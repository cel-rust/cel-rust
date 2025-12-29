use crate::common::types::Type;
use crate::common::value::Val;
use std::ops::Deref;
use std::time::Duration as StdDuration;

#[derive(Clone, Debug, PartialEq)]
pub struct Duration(StdDuration);

impl Duration {
    pub fn into_inner(self) -> StdDuration {
        self.0
    }

    pub fn inner(&self) -> &StdDuration {
        &self.0
    }
}

impl Deref for Duration {
    type Target = StdDuration;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Val for Duration {
    fn get_type(&self) -> Type<'_> {
        super::DURATION_TYPE
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Duration(self.0))
    }
}

impl From<StdDuration> for Duration {
    fn from(duration: StdDuration) -> Self {
        Self(duration)
    }
}

impl From<Duration> for StdDuration {
    fn from(duration: Duration) -> Self {
        duration.0
    }
}

impl TryFrom<Box<dyn Val>> for StdDuration {
    type Error = Box<dyn Val>;

    fn try_from(value: Box<dyn Val>) -> Result<Self, Self::Error> {
        if let Some(d) = value.downcast_ref::<Duration>() {
            return Ok(d.0);
        }
        Err(value)
    }
}

impl<'a> TryFrom<&'a dyn Val> for &'a StdDuration {
    type Error = &'a dyn Val;
    fn try_from(value: &'a dyn Val) -> Result<Self, Self::Error> {
        if let Some(d) = value.downcast_ref::<Duration>() {
            return Ok(&d.0);
        }
        Err(value)
    }
}

impl Default for Duration {
    fn default() -> Self {
        Duration(StdDuration::default())
    }
}

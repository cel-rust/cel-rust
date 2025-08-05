use crate::common::traits;
use crate::common::types::Type;
use crate::common::value::Val;
use std::any::Any;
use std::time::Duration as StdDuration;

#[derive(Debug)]
pub struct Duration(StdDuration);

impl Val for Duration {
    fn get_type(&self) -> Type<'_> {
        super::DURATION_TYPE
    }

    fn into_inner(self) -> Box<dyn Any> {
        Box::new(self.0)
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

impl traits::Adder for Duration {}

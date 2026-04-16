use crate::common::traits::{
    Adder, Comparer, Container, Divider, Indexer, Iterable, Modder, Multiplier, Negator, Sizer,
    Subtractor,
};
use crate::common::types::Type;
use std::any::Any;
use std::fmt::Debug;

pub trait Val: Any + Debug + Send + Sync {
    fn get_type<'a>(&self) -> &Type<'a>;

    fn as_adder(&self) -> Option<&dyn Adder> {
        None
    }

    fn as_comparer(&self) -> Option<&dyn Comparer> {
        None
    }

    fn as_container(&self) -> Option<&dyn Container> {
        None
    }

    fn as_divider(&self) -> Option<&dyn Divider> {
        None
    }

    fn as_indexer(&self) -> Option<&dyn Indexer> {
        None
    }

    fn into_indexer(self: Box<Self>) -> Option<Box<dyn Indexer>> {
        None
    }

    fn as_iterable(&self) -> Option<&dyn Iterable> {
        None
    }

    fn as_modder(&self) -> Option<&dyn Modder> {
        None
    }

    fn as_multiplier(&self) -> Option<&dyn Multiplier> {
        None
    }

    fn as_negator(&self) -> Option<&dyn Negator> {
        None
    }

    fn as_sizer(&self) -> Option<&dyn Sizer> {
        None
    }

    fn as_subtractor(&self) -> Option<&dyn Subtractor> {
        None
    }

    fn equals(&self, _other: &dyn Val) -> bool {
        false
    }

    fn clone_as_boxed(&self) -> Box<dyn Val>;
}

impl dyn Val {
    pub fn downcast_ref<T: Val>(&self) -> Option<&T> {
        <dyn Any>::downcast_ref::<T>(self)
    }
}

pub trait Downcast {
    type Error;

    fn downcast<T: Val>(self) -> Result<Box<T>, Self::Error>;
}

impl Downcast for Box<dyn Val> {
    type Error = Self;

    fn downcast<T: Val>(self) -> Result<Box<T>, Self> {
        if <dyn Any + 'static>::is::<T>(self.as_ref()) {
            return Ok(<Box<dyn Any>>::downcast::<T>(self).expect("we just tested it is!"));
        }
        Err(self)
    }
}

impl ToOwned for dyn Val {
    type Owned = Box<dyn Val>;

    fn to_owned(&self) -> Self::Owned {
        self.clone_as_boxed()
    }
}

impl PartialEq for dyn Val {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
mod test {
    use crate::common::types;
    use crate::common::types::CelString;
    use crate::common::value::Downcast;
    use crate::common::value::Val;
    use std::borrow::Cow;

    fn test(val: &dyn Val) -> bool {
        *val.get_type() == types::STRING_TYPE
    }

    #[test]
    fn test_cow() {
        let s1 = types::CelString::from("cel");
        let s2 = types::CelString::from("cel");
        let b: Box<dyn Val> = Box::new(s1);
        let cow: Cow<dyn Val> = Cow::Owned(b);
        let borrowed: Cow<dyn Val> = Cow::Borrowed(&s2);
        assert!(test(borrowed.as_ref()));
        assert!(test(cow.as_ref()));
        assert!(test(borrowed.clone().as_ref()));
        assert_eq!(cow.downcast_ref::<CelString>().unwrap().inner(), "cel");
        let boxed = cow.into_owned();
        let s: CelString = *boxed.downcast::<CelString>().unwrap();
        let s: String = s.into();
        assert_eq!(s.as_str(), "cel");
    }
}

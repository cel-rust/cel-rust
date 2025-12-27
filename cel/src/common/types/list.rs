use crate::common::traits::{Adder, Indexer};
use crate::common::types;
use crate::common::types::{CelInt, CelUInt, Type};
use crate::common::value::Val;
use crate::ExecutionError;
use std::borrow::Cow;
use std::ops::Deref;

#[derive(Debug)]
pub struct DefaultList(Vec<Box<dyn Val>>);

impl DefaultList {
    pub fn new(items: Vec<Box<dyn Val>>) -> Self {
        Self(items)
    }

    pub fn into_inner(self) -> Vec<Box<dyn Val>> {
        self.0
    }

    pub fn inner(&self) -> &[Box<dyn Val>] {
        &self.0
    }
}

impl Deref for DefaultList {
    type Target = [Box<dyn Val>];

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

impl Val for DefaultList {
    fn get_type(&self) -> Type<'_> {
        types::LIST_TYPE
    }

    fn as_adder(&self) -> Option<&dyn Adder> {
        todo!()
    }

    fn as_indexer(&self) -> Option<&dyn Indexer> {
        Some(self as &dyn Indexer)
    }

    fn into_indexer(self: Box<Self>) -> Option<Box<dyn Indexer>> {
        Some(self)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        let mut vec = Vec::with_capacity(self.0.len());
        for i in self.0.iter().map(|i| i.clone_as_boxed()) {
            vec.push(i);
        }
        Box::new(DefaultList(vec))
    }
}

impl Indexer for DefaultList {
    fn get<'a>(&'a self, idx: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        match idx.get_type() {
            types::INT_TYPE => {
                let idx: i64 = idx
                    .downcast_ref::<CelInt>()
                    .expect("We need an Indexer!")
                    .inner()
                    .clone();
                Ok(Cow::Borrowed(
                    self.0
                        .get(idx as usize)
                        .ok_or(ExecutionError::IndexOutOfBounds(idx.into()))?
                        .as_ref(),
                ))
            }
            _ => Err(ExecutionError::UnexpectedType {
                got: idx.get_type().runtime_type_name.to_string(),
                want: format!("{}", types::INT_TYPE.runtime_type_name),
            }),
        }
    }

    fn steal(self: Box<Self>, idx: &dyn Val) -> Result<Box<dyn Val>, ExecutionError> {
        let mut list = self;
        match idx.get_type() {
            types::INT_TYPE => {
                let idx: i64 = idx.downcast_ref::<CelInt>().unwrap().inner().clone();
                if idx < 0 || idx as usize >= list.0.len() {
                    return Err(ExecutionError::IndexOutOfBounds(idx.into()));
                }
                Ok(list.0.remove(idx as usize))
            }
            _ => Err(ExecutionError::UnexpectedType {
                got: idx.get_type().runtime_type_name.to_string(),
                want: format!("{}", types::INT_TYPE.runtime_type_name),
            }),
        }
    }
}

#[cfg(test)]
pub mod tests {
    use crate::common::traits::Indexer;
    use crate::common::types::list::DefaultList;
    use crate::common::types::{CelInt, CelString};
    use crate::common::value::Val;
    use crate::ExecutionError::{IndexOutOfBounds, UnexpectedType};

    #[test]
    fn list_has_indexer() {
        let list = Box::new(DefaultList(vec![]));
        assert!(list.as_indexer().is_some());
        assert!(list.into_indexer().is_some());
    }

    #[test]
    fn errs_out_of_index() {
        let list = DefaultList(vec![]);
        let idx: CelInt = 1.into();
        assert_eq!(
            Indexer::get(&list, &idx).err(),
            Some(IndexOutOfBounds(1.into()))
        );
        assert_eq!(
            Indexer::steal(list.into(), &idx).err(),
            Some(IndexOutOfBounds(1.into()))
        );
    }

    #[test]
    fn errs_unexpected_type() {
        let list = DefaultList(vec![]);
        let idx: CelString = "foo".into();
        assert_eq!(
            Indexer::get(&list, &idx).err(),
            Some(UnexpectedType {
                got: "string".to_string(),
                want: "int".to_string(),
            })
        );
        assert_eq!(
            Indexer::steal(list.into(), &idx).err(),
            Some(UnexpectedType {
                got: "string".to_string(),
                want: "int".to_string(),
            })
        );
    }

    #[test]
    fn get() {
        let val: CelString = "cel".into();
        let val: Box<dyn Val> = Box::new(val.clone());
        let list = DefaultList(vec![val]);
        let idx: CelInt = 0.into();
        assert!(Indexer::get(&list, &idx)
            .unwrap()
            .into_owned()
            .eq(&Into::<CelString>::into("cel")));
    }

    #[test]
    fn steal() {
        let val: CelString = "cel".into();
        let val: Box<dyn Val> = Box::new(val.clone());
        let list = DefaultList(vec![val]);
        let idx: CelInt = 0.into();
        assert!(Indexer::steal(list.into(), &idx)
            .unwrap()
            .eq(&Into::<CelString>::into("cel")));
    }
}

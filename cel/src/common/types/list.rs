use crate::common::traits::{Adder, Container, Indexer, Iterable, Sizer, Zeroer};
use crate::common::types::{CelInt, CelUInt, Kind, Type};
use crate::common::value::Val;
use crate::common::{traits, types};
use crate::ExecutionError;
use std::any::Any;
use std::borrow::Cow;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct DefaultList(Vec<Box<dyn Val>>);

impl DefaultList {
    pub fn into_inner(self) -> Vec<Box<dyn Val>> {
        self.0
    }

    pub fn inner(&self) -> &[Box<dyn Val>] {
        &self.0
    }

    fn clone(&self) -> Self {
        let mut vec = Vec::with_capacity(self.0.len());
        for i in self.0.iter().map(|i| i.clone_as_boxed()) {
            vec.push(i);
        }
        Self(vec)
    }
}

impl Deref for DefaultList {
    type Target = [Box<dyn Val>];

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

impl Val for DefaultList {
    fn get_type(&self) -> &Type {
        &types::LIST_TYPE
    }

    fn as_adder(&self) -> Option<&dyn Adder> {
        Some(self)
    }

    fn as_container(&self) -> Option<&dyn Container> {
        Some(self)
    }

    fn as_indexer(&self) -> Option<&dyn Indexer> {
        Some(self)
    }

    fn into_indexer(self: Box<Self>) -> Option<Box<dyn Indexer>> {
        Some(self)
    }

    fn as_iterable(&self) -> Option<&dyn Iterable> {
        Some(self)
    }

    fn as_sizer(&self) -> Option<&dyn Sizer> {
        Some(self)
    }

    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other
            .downcast_ref::<Self>()
            .is_some_and(|other| self.0 == other.0)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(self.clone())
    }
}

impl Adder for DefaultList {
    fn add<'a>(&'a self, rhs: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        let mut rhs = rhs
            .as_iterable()
            .ok_or(ExecutionError::NoSuchOverload)?
            .iter();
        let mut list = self.clone();
        while let Some(other) = rhs.next() {
            list.0.push(other.clone_as_boxed());
        }
        Ok(Cow::<dyn Val>::Owned(Box::new(list)))
    }
}

impl Container for DefaultList {
    fn contains(&self, value: &dyn Val) -> Result<bool, ExecutionError> {
        for i in &self.0 {
            if i.equals(value) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl Indexer for DefaultList {
    fn get<'a>(&'a self, idx: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        match idx.get_type().kind() {
            Kind::Int => {
                let idx: i64 = *idx
                    .downcast_ref::<CelInt>()
                    .ok_or(ExecutionError::NoSuchOverload)?
                    .inner();
                Ok(Cow::Borrowed(
                    self.0
                        .get(idx as usize)
                        .ok_or_else(|| ExecutionError::IndexOutOfBounds(idx.into()))?
                        .as_ref(),
                ))
            }
            Kind::UInt => {
                let idx: u64 = *idx
                    .downcast_ref::<CelUInt>()
                    .ok_or(ExecutionError::NoSuchOverload)?
                    .inner();
                Ok(Cow::Borrowed(
                    self.0
                        .get(idx as usize)
                        .ok_or_else(|| ExecutionError::IndexOutOfBounds(idx.into()))?
                        .as_ref(),
                ))
            }
            _ => Err(ExecutionError::UnexpectedType {
                got: idx.get_type().runtime_type_name.to_string(),
                want: format!(
                    "{}|{}",
                    types::INT_TYPE.runtime_type_name,
                    types::UINT_TYPE.runtime_type_name
                ),
            }),
        }
    }

    fn steal(self: Box<Self>, idx: &dyn Val) -> Result<Box<dyn Val>, ExecutionError> {
        let mut list = self;
        match idx.get_type().kind() {
            Kind::Int => {
                let idx: i64 = *idx
                    .downcast_ref::<CelInt>()
                    .ok_or(ExecutionError::NoSuchOverload)?
                    .inner();
                if idx < 0 || idx as usize >= list.0.len() {
                    return Err(ExecutionError::IndexOutOfBounds(idx.into()));
                }
                Ok(list.0.remove(idx as usize))
            }
            Kind::UInt => {
                let idx: u64 = *idx
                    .downcast_ref::<CelUInt>()
                    .ok_or(ExecutionError::NoSuchOverload)?
                    .inner();
                if idx as usize >= list.0.len() {
                    return Err(ExecutionError::IndexOutOfBounds(idx.into()));
                }
                Ok(list.0.remove(idx as usize))
            }
            _ => Err(ExecutionError::UnexpectedType {
                got: idx.get_type().runtime_type_name.to_string(),
                want: format!(
                    "{}|{}",
                    types::INT_TYPE.runtime_type_name,
                    types::UINT_TYPE.runtime_type_name
                ),
            }),
        }
    }
}

impl Iterable for DefaultList {
    fn iter<'a>(&'a self) -> Box<dyn super::traits::Iterator<'a> + 'a> {
        Box::new(SliceIterator::new(self.0.as_slice()))
    }
}

impl Sizer for DefaultList {
    fn size(&self) -> CelInt {
        (self.inner().len() as i64).into()
    }
}

impl Zeroer for DefaultList {
    fn is_zero_value(&self) -> bool {
        self.inner().is_empty()
    }
}

impl From<Vec<Box<dyn Val>>> for DefaultList {
    fn from(v: Vec<Box<dyn Val>>) -> Self {
        Self(v)
    }
}

impl TryFrom<Box<dyn Val>> for Vec<Box<dyn Val>> {
    type Error = Box<dyn Val>;

    fn try_from(value: Box<dyn Val>) -> Result<Self, Self::Error> {
        super::cast_boxed::<DefaultList>(value).map(|l| l.into_inner())
    }
}

impl<'a> TryFrom<&'a dyn Val> for &'a [Box<dyn Val>] {
    type Error = &'a dyn Val;

    fn try_from(value: &'a dyn Val) -> Result<Self, Self::Error> {
        if let Some(list) = <dyn Any>::downcast_ref::<DefaultList>(value) {
            return Ok(list.inner());
        }
        Err(value)
    }
}

pub struct SliceIterator<'a> {
    list: &'a [Box<dyn Val>],
    pos: usize,
}

impl<'a> SliceIterator<'a> {
    fn new(list: &'a [Box<dyn Val>]) -> Self {
        Self { list, pos: 0 }
    }
}

impl<'a> traits::Iterator<'a> for SliceIterator<'a> {
    fn next(&mut self) -> Option<&'a dyn Val> {
        if self.pos >= self.list.len() {
            None
        } else {
            let r = &self.list[self.pos];
            self.pos += 1;
            Some(r.as_ref())
        }
    }
}

/// A mutable list that shares its backing storage across clones.
///
/// `MutableList` is an internal helper used by the comprehension evaluator
/// (see [`crate::objects::Value::resolve_val`]) to build up the accumulator
/// of `map` / `filter` comprehensions in place, avoiding the quadratic
/// clone-on-add cost that a normal [`DefaultList`] would incur when the
/// comprehension expands to `@result = @result + [expr]` on every iteration.
///
/// Only the surface the comprehension actually invokes on the accumulator
/// is implemented — `Val` + [`Adder`]. `DefaultList` stays a full-fledged
/// value type (with `Container`/`Indexer`/`Iterable`/`Sizer`/`Zeroer`);
/// `MutableList` never leaves this scope, so those trait impls would be
/// dead code.
///
/// It is not exposed to user code and should never be produced by
/// user-defined overloads or programs. When a comprehension completes, the
/// evaluator converts the accumulator back to a [`DefaultList`] via
/// [`MutableList::to_immutable`].
#[derive(Debug, Default)]
#[doc(hidden)]
pub struct MutableList {
    inner: Arc<Mutex<Vec<Box<dyn Val>>>>,
}

impl MutableList {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::with_capacity(cap))),
        }
    }

    /// Converts the mutable list into an immutable [`DefaultList`], reusing
    /// the backing storage if this handle is the only one still alive.
    pub fn to_immutable(self) -> DefaultList {
        match Arc::try_unwrap(self.inner) {
            Ok(mutex) => DefaultList(mutex.into_inner().expect("mutable list mutex poisoned")),
            Err(shared) => {
                let guard = shared.lock().expect("mutable list mutex poisoned");
                let mut out = Vec::with_capacity(guard.len());
                for v in guard.iter() {
                    out.push(v.clone_as_boxed());
                }
                DefaultList(out)
            }
        }
    }

    fn share(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }

    #[cfg(test)]
    fn len_for_test(&self) -> usize {
        self.inner
            .lock()
            .expect("mutable list mutex poisoned")
            .len()
    }
}

// `MutableList` only implements the Val + trait surface the comprehension
// evaluator actually invokes on the accumulator:
//   * `get_type` / `clone_as_boxed` — mandatory on every `Val`, and
//     `clone_as_boxed` fires on every loop iteration when the accumulator
//     is re-stored in the child context (the `Arc` inside means it's a
//     refcount bump, not a buffer copy).
//   * `as_adder` — invoked by the ADD dispatch every time the loop step
//     evaluates `@result + [expr]`.
// The other trait accessors (`as_iterable`, `as_indexer`, `as_container`,
// `as_sizer`, `as_zeroer`) never fire because the accumulator identifier
// `@result` is not typeable in CEL source, so user expressions can never
// call `size()`, iterate, index, or use `in` on it; and the accumulator
// is always frozen to `DefaultList` before it leaves the comprehension
// scope, so no user code ever sees a `MutableList` either. Same reasoning
// leaves `equals` at the default `false` — no one compares mutable lists.
impl Val for MutableList {
    fn get_type(&self) -> &Type {
        &types::LIST_TYPE
    }

    fn as_adder(&self) -> Option<&dyn Adder> {
        Some(self)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(self.share())
    }
}

impl Adder for MutableList {
    fn add<'a>(&'a self, rhs: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        let iter = rhs
            .as_iterable()
            .ok_or(ExecutionError::NoSuchOverload)?
            .iter();
        {
            let mut inner = self.inner.lock().expect("mutable list mutex poisoned");
            let mut items = iter;
            while let Some(other) = items.next() {
                inner.push(other.clone_as_boxed());
            }
        }
        Ok(Cow::Borrowed(self as &dyn Val))
    }
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    env.add_overload(
        "size",
        "size_list",
        vec![super::LIST_TYPE],
        traits::adapter::sizer_size,
    )
    .expect("Must be unique id");
    env.add_member_overload(
        "size",
        "list_size",
        super::LIST_TYPE,
        vec![],
        traits::adapter::sizer_size,
    )
    .expect("Must be unique id");
}

#[cfg(test)]
pub mod tests {
    use crate::common::traits::Indexer;
    use crate::common::types::list::DefaultList;
    use crate::common::types::{CelInt, CelString};
    use crate::common::value::Val;
    use crate::ExecutionError::{IndexOutOfBounds, UnexpectedType};
    use std::borrow::Cow;

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
                want: "int|uint".to_string(),
            })
        );
        assert_eq!(
            Indexer::steal(list.into(), &idx).err(),
            Some(UnexpectedType {
                got: "string".to_string(),
                want: "int|uint".to_string(),
            })
        );
    }

    #[test]
    fn get() {
        let val: CelString = "cel".into();
        let val: Box<dyn Val> = Box::new(val.clone());
        let list = DefaultList(vec![val]);
        let idx: CelInt = 0.into();
        let expected = Cow::<dyn Val>::Owned(Box::new(Into::<CelString>::into("cel")));
        assert_eq!(Indexer::get(&list, &idx), Ok(expected));
    }

    #[test]
    fn steal() {
        let val: CelString = "cel".into();
        let val: Box<dyn Val> = Box::new(val.clone());
        let list = DefaultList(vec![val]);
        let idx: CelInt = 0.into();
        let expected: Box<dyn Val> = Box::new(Into::<CelString>::into("cel"));
        assert_eq!(Indexer::steal(list.into(), &idx), Ok(expected));
    }

    #[test]
    fn try_into_vec() {
        let v1: Box<dyn Val> = Box::new(Into::<CelString>::into("cel"));
        let v2: Box<dyn Val> = Box::new(Into::<CelString>::into("rust"));
        let list: Box<dyn Val> = Box::new(DefaultList(vec![v1, v2]));
        let list: Vec<Box<dyn Val>> = list.try_into().unwrap();
        assert_eq!(list[0].downcast_ref::<CelString>().unwrap().inner(), "cel");
        assert_eq!(list[1].downcast_ref::<CelString>().unwrap().inner(), "rust");
    }

    #[test]
    fn try_into_slice() {
        let v1: Box<dyn Val> = Box::new(Into::<CelString>::into("cel"));
        let v2: Box<dyn Val> = Box::new(Into::<CelString>::into("rust"));
        let list: Box<dyn Val> = Box::new(DefaultList(vec![v1, v2]));
        let list: &[Box<dyn Val>] = list.as_ref().try_into().unwrap();
        assert_eq!(list[0].downcast_ref::<CelString>().unwrap().inner(), "cel");
        assert_eq!(list[1].downcast_ref::<CelString>().unwrap().inner(), "rust");
    }

    use crate::common::traits::Adder;
    use crate::common::types::list::MutableList;

    fn box_val<V: Val>(v: V) -> Box<dyn Val> {
        Box::new(v)
    }

    #[test]
    fn mutable_list_add_extends_in_place_across_clones() {
        let m = MutableList::with_capacity(4);
        let clone_box = m.clone_as_boxed();
        let clone = clone_box.downcast_ref::<MutableList>().unwrap();

        m.add(&DefaultList::from(vec![box_val(CelInt::from(1i64))]))
            .unwrap();
        m.add(&DefaultList::from(vec![box_val(CelInt::from(2i64))]))
            .unwrap();
        clone
            .add(&DefaultList::from(vec![box_val(CelInt::from(3i64))]))
            .unwrap();

        // Both handles observe the same growing buffer.
        assert_eq!(m.len_for_test(), 3);
        assert_eq!(clone.len_for_test(), 3);
    }

    #[test]
    fn mutable_list_to_immutable_preserves_order() {
        let m = MutableList::with_capacity(0);
        for i in 0..5i64 {
            let rhs = DefaultList::from(vec![box_val(CelInt::from(i))]);
            m.add(&rhs).unwrap();
        }
        let imm = m.to_immutable();
        assert_eq!(imm.inner().len(), 5);
        for (i, v) in imm.inner().iter().enumerate() {
            assert_eq!(*v.downcast_ref::<CelInt>().unwrap().inner(), i as i64);
        }
    }
}

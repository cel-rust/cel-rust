use crate::common::traits::{Adder, Container, Indexer, Iterable, Sizer, Zeroer};
use crate::common::types::{CelInt, CelUInt, Kind, Type};
use crate::common::value::{Builtin, BuiltinRef, CowVal, Val};
use crate::common::{traits, types};
use crate::ExecutionError;
use std::ops::Deref;

/// A CEL list whose elements may borrow data for `'v`.
#[derive(Debug, Default)]
pub struct DefaultList<'v>(Vec<Box<dyn Val + 'v>>);

impl<'v> DefaultList<'v> {
    pub fn into_inner(self) -> Vec<Box<dyn Val + 'v>> {
        self.0
    }

    pub fn inner(&self) -> &[Box<dyn Val + 'v>] {
        &self.0
    }
}

impl<'v> Clone for DefaultList<'v> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<'v> Deref for DefaultList<'v> {
    type Target = [Box<dyn Val + 'v>];

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

impl<'v> Val for DefaultList<'v> {
    fn get_type(&self) -> &Type {
        <Self as Val>::cel_type()
    }

    fn cel_type() -> &'static Type {
        &types::LIST_TYPE
    }

    fn as_adder<'b, 'w>(&'b self) -> Option<&'b (dyn Adder + 'w)>
    where
        Self: 'w,
    {
        Some(self)
    }

    fn as_container(&self) -> Option<&dyn Container> {
        Some(self)
    }

    fn as_indexer<'b, 'w>(&'b self) -> Option<&'b (dyn Indexer + 'w)>
    where
        Self: 'w,
    {
        Some(self)
    }

    fn into_indexer<'w>(self: Box<Self>) -> Option<Box<dyn Indexer + 'w>>
    where
        Self: 'w,
    {
        Some(self)
    }

    fn as_iterable<'b, 'w>(&'b self) -> Option<&'b (dyn Iterable + 'w)>
    where
        Self: 'w,
    {
        Some(self)
    }

    fn as_sizer(&self) -> Option<&dyn Sizer> {
        Some(self)
    }

    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other.downcast_ref::<DefaultList>().is_some_and(|other| {
            self.0.len() == other.0.len()
                && self
                    .0
                    .iter()
                    .zip(other.0.iter())
                    .all(|(a, b)| a.equals(b.as_ref()))
        })
    }

    fn clone_as_boxed<'w>(&self) -> Box<dyn Val + 'w>
    where
        Self: 'w,
    {
        Box::new(self.clone())
    }

    fn as_builtin<'b, 'w>(&'b self) -> BuiltinRef<'b, 'w>
    where
        Self: 'w,
    {
        BuiltinRef::List(self)
    }

    fn into_builtin<'w>(self: Box<Self>) -> Option<Builtin<'w>>
    where
        Self: 'w,
    {
        Some(Builtin::List(*self))
    }
}

impl<'v> Adder for DefaultList<'v> {
    fn add<'b, 'w>(&'b self, rhs: &(dyn Val + 'w)) -> Result<CowVal<'b, 'w>, ExecutionError>
    where
        Self: 'w,
    {
        let mut rhs = rhs
            .as_iterable()
            .ok_or_else(|| ExecutionError::unsupported_binary_operator("add", self, rhs))?
            .iter();
        let mut list: Vec<Box<dyn Val + 'w>> = self.0.iter().map(|i| i.clone_as_boxed()).collect();
        while let Some(other) = rhs.next() {
            list.push(other.clone_as_boxed());
        }
        Ok(CowVal::owned(DefaultList(list)))
    }
}

impl Container for DefaultList<'_> {
    fn contains(&self, value: &dyn Val) -> Result<bool, ExecutionError> {
        for i in &self.0 {
            if i.equals(value) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn index(list: &dyn Val, idx: &dyn Val) -> Result<usize, ExecutionError> {
    match idx.get_type().kind() {
        Kind::Int => {
            let idx: i64 = *idx
                .downcast_ref::<CelInt>()
                .ok_or_else(|| ExecutionError::overload_for_values("_[_]", [list, idx], false))?
                .inner();
            usize::try_from(idx).map_err(|_| ExecutionError::IndexOutOfBounds(idx.into()))
        }
        Kind::UInt => {
            let idx: u64 = *idx
                .downcast_ref::<CelUInt>()
                .ok_or_else(|| ExecutionError::overload_for_values("_[_]", [list, idx], false))?
                .inner();
            usize::try_from(idx).map_err(|_| ExecutionError::IndexOutOfBounds(idx.into()))
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

fn out_of_bounds(idx: &dyn Val) -> ExecutionError {
    ExecutionError::IndexOutOfBounds(idx.try_into().unwrap_or(crate::Value::Null))
}

impl<'v> Indexer for DefaultList<'v> {
    fn get<'b, 'w>(&'b self, idx: &dyn Val) -> Result<CowVal<'b, 'w>, ExecutionError>
    where
        Self: 'w,
    {
        let i = index(self, idx)?;
        self.0
            .get(i)
            .map(|v| CowVal::Borrowed(v.as_ref()))
            .ok_or_else(|| out_of_bounds(idx))
    }

    fn steal<'w>(self: Box<Self>, idx: &dyn Val) -> Result<Box<dyn Val + 'w>, ExecutionError>
    where
        Self: 'w,
    {
        let mut list = self;
        let i = index(list.as_ref(), idx)?;
        if i >= list.0.len() {
            return Err(out_of_bounds(idx));
        }
        Ok(list.0.swap_remove(i))
    }
}

impl<'v> Iterable for DefaultList<'v> {
    fn iter<'b, 'w>(&'b self) -> Box<dyn traits::Iterator<'b, 'w> + 'b>
    where
        Self: 'w,
    {
        Box::new(SliceIterator::new(self.0.as_slice()))
    }
}

impl Sizer for DefaultList<'_> {
    fn size(&self) -> CelInt {
        (self.inner().len() as i64).into()
    }
}

impl Zeroer for DefaultList<'_> {
    fn is_zero_value(&self) -> bool {
        self.inner().is_empty()
    }
}

impl<'v> From<Vec<Box<dyn Val + 'v>>> for DefaultList<'v> {
    fn from(v: Vec<Box<dyn Val + 'v>>) -> Self {
        Self(v)
    }
}

impl<'v> TryFrom<Box<dyn Val + 'v>> for Vec<Box<dyn Val + 'v>> {
    type Error = Box<dyn Val + 'v>;

    fn try_from(value: Box<dyn Val + 'v>) -> Result<Self, Self::Error> {
        match super::into_builtin(value) {
            Ok(Builtin::List(l)) => Ok(l.into_inner()),
            Ok(other) => Err(other.into_boxed()),
            Err(value) => Err(value),
        }
    }
}

impl<'a, 'v> TryFrom<&'a (dyn Val + 'v)> for &'a [Box<dyn Val + 'v>] {
    type Error = &'a (dyn Val + 'v);

    fn try_from(value: &'a (dyn Val + 'v)) -> Result<Self, Self::Error> {
        if let Some(list) = value.downcast_ref::<DefaultList>() {
            return Ok(list.inner());
        }
        Err(value)
    }
}

pub struct SliceIterator<'b, 'v> {
    list: &'b [Box<dyn Val + 'v>],
    pos: usize,
}

impl<'b, 'v> SliceIterator<'b, 'v> {
    fn new(list: &'b [Box<dyn Val + 'v>]) -> Self {
        Self { list, pos: 0 }
    }
}

impl<'b, 'v: 'w, 'w> traits::Iterator<'b, 'w> for SliceIterator<'b, 'v> {
    fn next(&mut self) -> Option<&'b (dyn Val + 'w)> {
        if self.pos >= self.list.len() {
            None
        } else {
            let r = &self.list[self.pos];
            self.pos += 1;
            Some(r.as_ref())
        }
    }
}

/// A list under construction, appended to in place.
///
/// `MutableList` is an internal helper used by the comprehension evaluator
/// (see [`crate::objects::Value::resolve_val`]) to build up the accumulator
/// of `map` / `filter` comprehensions, avoiding the quadratic
/// clone-on-add cost that a [`DefaultList`] would incur when the
/// comprehension expands to `@result = @result + [expr]` on every iteration.
///
/// It is deliberately not a [`Val`]: the evaluator owns it for the duration
/// of the loop and never binds it to a variable, so no expression can observe
/// it half-built. Appending goes through [`MutableList::extend`] rather than
/// [`Adder`], because growing the list in place is only sound for elements
/// that live as long as the list does (`'v`), which the generic
/// `Adder::add<'b, 'w>` cannot express. When the loop completes, the
/// evaluator converts it into a [`DefaultList`] via
/// [`MutableList::to_immutable`].
#[derive(Debug, Default)]
#[doc(hidden)]
pub struct MutableList<'v>(Vec<Box<dyn Val + 'v>>);

impl<'v> MutableList<'v> {
    pub fn with_capacity(cap: usize) -> Self {
        Self(Vec::with_capacity(cap))
    }

    /// Appends a copy of every element of `rhs`, which must be a list.
    pub fn extend(&mut self, rhs: &(dyn Val + 'v)) -> Result<(), ExecutionError> {
        let mut items = rhs
            .as_iterable()
            .ok_or_else(|| ExecutionError::UnexpectedType {
                got: rhs.get_type().name().to_owned(),
                want: "iterable".to_owned(),
            })?
            .iter();
        while let Some(item) = items.next() {
            self.0.push(item.clone_as_boxed());
        }
        Ok(())
    }

    /// Converts the mutable list into an immutable [`DefaultList`], reusing
    /// the backing storage.
    pub fn to_immutable(self) -> DefaultList<'v> {
        DefaultList(self.0)
    }

    #[cfg(test)]
    fn len_for_test(&self) -> usize {
        self.0.len()
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
    use crate::common::types::{self, CelInt, CelString, Type};
    use crate::common::value::{CowVal, Val};
    use crate::ExecutionError;
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
        let expected: CowVal<'_, '_> = CowVal::owned(Into::<CelString>::into("cel"));
        assert_eq!(Indexer::get(&list, &idx), Ok(expected));
    }

    #[test]
    fn numeric_kind_without_numeric_value_returns_overload_error() {
        #[derive(Debug, Clone)]
        struct NumericKindOnly(&'static Type);

        impl Val for NumericKindOnly {
            fn get_type(&self) -> &Type {
                self.0
            }

            fn cel_type() -> &'static Type {
                panic!("the type under test is the one this value was built with")
            }

            fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v> {
                Box::new(self.clone())
            }
        }

        for ty in [&types::INT_TYPE, &types::UINT_TYPE] {
            let idx = NumericKindOnly(ty);
            let list = DefaultList::default();
            let expected = ExecutionError::no_such_overload(
                "_[_]",
                vec!["list".to_owned(), ty.name().to_owned()],
            );
            assert_eq!(Indexer::get(&list, &idx).err(), Some(expected.clone()));
            assert_eq!(Indexer::steal(Box::new(list), &idx).err(), Some(expected));
        }
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

    use crate::common::types::list::MutableList;

    fn box_val<'v, V: Val + 'v>(v: V) -> Box<dyn Val + 'v> {
        Box::new(v)
    }

    #[test]
    fn mutable_list_extends_in_place() {
        let mut m = MutableList::with_capacity(4);

        m.extend(&DefaultList::from(vec![box_val(CelInt::from(1i64))]))
            .unwrap();
        m.extend(&DefaultList::from(vec![
            box_val(CelInt::from(2i64)),
            box_val(CelInt::from(3i64)),
        ]))
        .unwrap();

        assert_eq!(m.len_for_test(), 3);
    }

    #[test]
    fn mutable_list_extend_requires_a_list() {
        let mut m = MutableList::with_capacity(0);
        assert_eq!(
            m.extend(&CelInt::from(1i64)),
            Err(ExecutionError::UnexpectedType {
                got: "int".to_owned(),
                want: "iterable".to_owned(),
            })
        );
    }

    #[test]
    fn mutable_list_extend_keeps_borrowed_elements() {
        let owned = String::from("cel");
        let mut m = MutableList::with_capacity(1);
        m.extend(&DefaultList::from(vec![box_val(CelString::from(
            owned.as_str(),
        ))]))
        .unwrap();
        let imm = m.to_immutable();
        let s = imm.inner()[0].downcast_ref::<CelString>().unwrap();
        assert!(std::ptr::eq(s.inner(), owned.as_str()));
    }

    #[test]
    fn mutable_list_to_immutable_preserves_order() {
        let mut m = MutableList::with_capacity(0);
        for i in 0..5i64 {
            let rhs = DefaultList::from(vec![box_val(CelInt::from(i))]);
            m.extend(&rhs).unwrap();
        }
        let imm = m.to_immutable();
        assert_eq!(imm.inner().len(), 5);
        for (i, v) in imm.inner().iter().enumerate() {
            assert_eq!(*v.downcast_ref::<CelInt>().unwrap().inner(), i as i64);
        }
    }
}

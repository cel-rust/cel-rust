use crate::common::traits::{Comparer, Zeroer};
use crate::common::types::{CelString, Type};
use crate::common::value::{StaticVal, Val};
use crate::ExecutionError;
use std::any::Any;
use std::ops::Deref;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
/// A CEL `bool`, of type [`BOOL_TYPE`](super::BOOL_TYPE).
///
/// It orders `false` before `true`, and is only equal to a `bool`. `!`, `&&`,
/// `||` and the condition of `? :` take one; it has no [`Negator`], so `-true`
/// is an error.
///
/// [`Negator`]: crate::common::traits::Negator
pub struct Bool(bool);

impl Bool {
    /// `true`.
    pub const TRUE: Bool = Bool(true);
    /// `false`.
    pub const FALSE: Bool = Bool(false);

    /// `!self`.
    pub fn negate(&self) -> Self {
        Self(!self.0)
    }

    /// The `bool`.
    pub fn into_inner(self) -> bool {
        self.0
    }

    /// A reference to the `bool`.
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
    fn get_type(&self) -> &Type {
        <Self as Val>::cel_type()
    }

    fn cel_type() -> &'static Type {
        &super::BOOL_TYPE
    }

    fn as_comparer(&self) -> Option<&dyn Comparer> {
        Some(self)
    }

    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other.downcast_ref::<Self>().is_some_and(|a| self.0 == a.0)
    }

    fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v>
    where
        Self: 'v,
    {
        Box::new(*self)
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

impl StaticVal for Bool {}

impl Comparer for Bool {
    fn compare(&self, rhs: &dyn Val) -> Result<std::cmp::Ordering, crate::ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Bool>() {
            Ok(self.0.cmp(&rhs.0))
        } else {
            Err(ExecutionError::values_not_comparable(self, rhs))
        }
    }
}

impl Zeroer for Bool {
    fn is_zero_value(&self) -> bool {
        !self.0
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

impl<'v> TryFrom<Box<dyn Val + 'v>> for bool {
    type Error = Box<dyn Val + 'v>;

    fn try_from(value: Box<dyn Val + 'v>) -> Result<Self, Self::Error> {
        if let Some(b) = value.downcast_ref::<Bool>() {
            return Ok(b.0);
        }
        Err(value)
    }
}

impl<'a, 'v> TryFrom<&'a (dyn Val + 'v)> for &'a bool {
    type Error = &'a (dyn Val + 'v);

    fn try_from(value: &'a (dyn Val + 'v)) -> Result<Self, Self::Error> {
        if let Some(b) = value.downcast_ref::<Bool>() {
            return Ok(&b.0);
        }
        Err(value)
    }
}

fn bool_from_bool(this: &Bool) -> Bool {
    *this
}

fn bool_from_string(this: &CelString<'_>) -> Result<Bool, ExecutionError> {
    // Same accepted set as cel-cpp and cel-java. cel-go's strconv.ParseBool also takes "T" and "F".
    match this.inner() {
        "1" | "t" | "true" | "TRUE" | "True" => Ok(Bool(true)),
        "0" | "f" | "false" | "FALSE" | "False" => Ok(Bool(false)),
        _ => Err(ExecutionError::FunctionError {
            function: "bool".to_owned(),
            message: "Type conversion error from 'string' to 'bool'".to_owned(),
        }),
    }
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    env.add_type(crate::common::types::BOOL_TYPE)
        .expect("Must be unique");
    crate::add_overload!(env, fn bool_from_bool: (Bool) -> Bool,
        name = "bool", id = "bool_to_bool")
    .expect("Must be unique id");
    crate::add_overload!(env, fn bool_from_string: (CelString) -> Result<Bool>,
        name = "bool", id = "string_to_bool")
    .expect("Must be unique id");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types;
    use crate::common::types::Kind;
    use crate::{Context, Program};

    #[test]
    fn test_from() {
        let value: Bool = true.into();
        let v: bool = value.into();
        assert!(v)
    }

    #[test]
    fn test_type() {
        let value = Bool(true);
        assert_eq!(*value.get_type(), types::BOOL_TYPE);
        assert_eq!(value.get_type().kind, Kind::Boolean);
    }

    #[test]
    fn test_unary_minus_is_not_logical_not() {
        let context = crate::Context::default();
        let program = crate::Program::compile("-false").unwrap();
        assert!(matches!(
            program.execute(&context),
            Err(ExecutionError::NoSuchOverload(_))
        ));
        let program = crate::Program::compile("!false").unwrap();
        assert_eq!(program.execute(&context), Ok(true.into()));
    }

    fn eval(expr: &str) -> crate::objects::ResolveResult {
        Program::compile(expr).unwrap().execute(&Context::default())
    }

    #[test]
    fn test_conversion() {
        assert_eq!(eval("bool(true)"), Ok(true.into()));
        assert_eq!(eval("bool(false)"), Ok(false.into()));
        for s in ["1", "t", "true", "TRUE", "True"] {
            assert_eq!(eval(&format!("bool('{s}')")), Ok(true.into()), "{s}");
        }
        for s in ["0", "f", "false", "FALSE", "False"] {
            assert_eq!(eval(&format!("bool('{s}')")), Ok(false.into()), "{s}");
        }
        for s in ["TrUe", "FaLsE", "T", "F", "yes", "", " true"] {
            assert_eq!(
                eval(&format!("bool('{s}')")),
                Err(ExecutionError::FunctionError {
                    function: "bool".to_owned(),
                    message: "Type conversion error from 'string' to 'bool'".to_owned(),
                }),
                "{s}"
            );
        }
    }
}

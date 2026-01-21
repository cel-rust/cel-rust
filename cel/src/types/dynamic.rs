use crate::Value;
use std::marker::PhantomData;
use std::ptr::NonNull;

pub struct Vtable {
    pub materialize: unsafe fn(*const ()) -> Value<'static>,
    pub field: unsafe fn(*const (), &str) -> Option<Value<'static>>,
    pub debug: unsafe fn(*const (), &mut std::fmt::Formatter<'_>) -> std::fmt::Result,
}

pub trait DynamicValueVtable {
    fn vtable() -> &'static Vtable;
}

pub fn maybe_materialize<T: DynamicType + DynamicValueVtable>(t: &T) -> Value<'_> {
    if t.auto_materialize() {
        t.materialize()
    } else {
        Value::Dynamic(DynamicValue::new(t))
    }
}

pub fn always_materialize(t: Value) -> Value {
    if let Value::Dynamic(d) = t {
        d.materialize()
    } else {
        t
    }
}

pub trait DynamicType: DynamicValueVtable + std::fmt::Debug + Send + Sync {
    // If the value can be freely converted to a Value, do so.
    // This is anything but list/map
    fn auto_materialize(&self) -> bool {
        false
    }

    // Convert this dynamic value into a proper value
    fn materialize(&self) -> Value<'_>;

    fn field(&self, _field: &str) -> Option<Value<'_>> {
        None
    }
}

pub struct DynamicValue<'a> {
    data: NonNull<()>,
    vtable: &'static Vtable,
    _marker: PhantomData<fn() -> &'a ()>,
}

impl<'a> DynamicValue<'a> {
    pub fn new<T>(t: &'a T) -> Self
    where
        T: DynamicType + DynamicValueVtable,
    {
        DynamicValue {
            data: NonNull::from(t).cast(),
            vtable: T::vtable(),
            _marker: PhantomData,
        }
    }

    pub fn materialize(&self) -> Value<'a> {
        unsafe {
            // Safety: The vtable returns Value<'static>, but we transmute it to Value<'a>
            // This is sound because:
            // 1. The actual data pointer points to data that lives for 'a
            // 2. PhantomData tracks the correct lifetime 'a
            // 3. The implementation only returns data borrowed from self
            std::mem::transmute((self.vtable.materialize)(self.data.as_ptr()))
        }
    }

    pub fn field(&self, field: &str) -> Option<Value<'a>> {
        unsafe {
            // Safety: Same reasoning as materialize()
            std::mem::transmute((self.vtable.field)(self.data.as_ptr(), field))
        }
    }
}

impl<'a> Clone for DynamicValue<'a> {
    fn clone(&self) -> Self {
        DynamicValue {
            data: self.data,
            vtable: self.vtable,
            _marker: PhantomData,
        }
    }
}

impl<'a> std::fmt::Debug for DynamicValue<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe { (self.vtable.debug)(self.data.as_ptr(), f) }
    }
}

// Safety: DynamicValue is Send/Sync if the underlying type is Send + Sync (enforced by trait bound)
unsafe impl<'a> Send for DynamicValue<'a> {}
unsafe impl<'a> Sync for DynamicValue<'a> {}

/// Implement DynamicValueVtable for a type that already implements DynamicType.
/// Use this for foreign types where you can't use #[derive(DynamicType)].
///
/// # Example
///
/// ```ignore
/// impl DynamicType for &str {
///     fn materialize(&self) -> Value<'_> {
///         Value::from(*self)
///     }
///     fn auto_materialize(&self) -> bool {
///         true
///     }
/// }
/// impl_dynamic_vtable!(&str);
/// ```
#[macro_export]
macro_rules! impl_dynamic_vtable {
    ($ty:ty) => {
        impl $crate::types::dynamic::DynamicValueVtable for $ty {
            fn vtable() -> &'static $crate::types::dynamic::Vtable {
                use std::sync::OnceLock;
                static VTABLE: OnceLock<$crate::types::dynamic::Vtable> = OnceLock::new();
                VTABLE.get_or_init(|| {
                    unsafe fn materialize_impl(ptr: *const ()) -> $crate::Value<'static> {
                        unsafe {
                            let this = &*(ptr as *const $ty);
                            ::std::mem::transmute(
                                <$ty as $crate::types::dynamic::DynamicType>::materialize(this),
                            )
                        }
                    }

                    unsafe fn field_impl(
                        ptr: *const (),
                        field: &str,
                    ) -> ::core::option::Option<$crate::Value<'static>> {
                        unsafe {
                            let this = &*(ptr as *const $ty);
                            ::std::mem::transmute(
                                <$ty as $crate::types::dynamic::DynamicType>::field(this, field),
                            )
                        }
                    }

                    unsafe fn debug_impl(
                        ptr: *const (),
                        f: &mut ::std::fmt::Formatter<'_>,
                    ) -> ::std::fmt::Result {
                        unsafe {
                            let this = &*(ptr as *const $ty);
                            ::std::fmt::Debug::fmt(this, f)
                        }
                    }

                    $crate::types::dynamic::Vtable {
                        materialize: materialize_impl,
                        field: field_impl,
                        debug: debug_impl,
                    }
                })
            }
        }
    };
}

// Primitive type implementations

// &str - auto-materializes to String value
impl DynamicType for &str {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(*self)
    }
}
impl_dynamic_vtable!(&str);

// String - auto-materializes to String value
impl DynamicType for String {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(self.as_str())
    }
}
impl_dynamic_vtable!(String);

// bool - auto-materializes to Bool value
impl DynamicType for bool {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(*self)
    }
}
impl_dynamic_vtable!(bool);

// i64 - auto-materializes to Int value
impl DynamicType for i64 {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(*self)
    }
}
impl_dynamic_vtable!(i64);

// u64 - auto-materializes to Int value (as i64)
impl DynamicType for u64 {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(*self as i64)
    }
}
impl_dynamic_vtable!(u64);

// i32 - auto-materializes to Int value
impl DynamicType for i32 {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(*self as i64)
    }
}
impl_dynamic_vtable!(i32);

// u32 - auto-materializes to Int value
impl DynamicType for u32 {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(*self as i64)
    }
}
impl_dynamic_vtable!(u32);

// f64 - auto-materializes to Float value
impl DynamicType for f64 {
    fn auto_materialize(&self) -> bool {
        true
    }

    fn materialize(&self) -> Value<'_> {
        Value::from(*self)
    }
}
impl_dynamic_vtable!(f64);

// Collection types - these do NOT auto-materialize since they're complex structures

// HashMap<String, String> - materializes to Map value
impl DynamicType for std::collections::HashMap<String, String> {
    fn auto_materialize(&self) -> bool {
        false // Maps are complex, don't auto-materialize
    }

    fn materialize(&self) -> Value<'_> {
        let mut map = vector_map::VecMap::with_capacity(self.len());
        for (k, v) in self.iter() {
            map.insert(
                crate::objects::KeyRef::from(k.as_str()),
                Value::from(v.as_str()),
            );
        }
        Value::Map(crate::objects::MapValue::Borrow(map))
    }

    fn field(&self, field: &str) -> Option<Value<'_>> {
        self.get(field).map(|v| Value::from(v.as_str()))
    }
}
impl_dynamic_vtable!(std::collections::HashMap<String, String>);

// &HashMap<String, String> - reference to HashMap
impl DynamicType for &std::collections::HashMap<String, String> {
    fn auto_materialize(&self) -> bool {
        false
    }

    fn materialize(&self) -> Value<'_> {
        let mut map = vector_map::VecMap::with_capacity(self.len());
        for (k, v) in self.iter() {
            map.insert(
                crate::objects::KeyRef::from(k.as_str()),
                Value::from(v.as_str()),
            );
        }
        Value::Map(crate::objects::MapValue::Borrow(map))
    }

    fn field(&self, field: &str) -> Option<Value<'_>> {
        self.get(field).map(|v| Value::from(v.as_str()))
    }
}
impl_dynamic_vtable!(&std::collections::HashMap<String, String>);

// Vec<String> - materializes to List value
impl DynamicType for Vec<String> {
    fn auto_materialize(&self) -> bool {
        false // Lists are complex, don't auto-materialize
    }

    fn materialize(&self) -> Value<'_> {
        let items: Vec<Value<'static>> = self.iter().map(|s| Value::from(s.clone())).collect();
        Value::List(crate::objects::ListValue::Owned(items.into()))
    }
}
impl_dynamic_vtable!(Vec<String>);

// &[String] - slice of Strings
impl DynamicType for &[String] {
    fn auto_materialize(&self) -> bool {
        false
    }

    fn materialize(&self) -> Value<'_> {
        let items: Vec<Value<'static>> = self.iter().map(|s| Value::from(s.clone())).collect();
        Value::List(crate::objects::ListValue::Owned(items.into()))
    }
}
impl_dynamic_vtable!(&[String]);

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

    fn field(&self, field: &str) -> Option<Value<'_>> {
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

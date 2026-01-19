use crate::extractors::Function;
use crate::objects::ObjectType;
use crate::Value;
use indexmap::map::Entry;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::{Arc, LazyLock};

#[linkme::distributed_slice]
pub static REGISTERED_TYPES: [fn(&mut Registration)];
static VTABLES: LazyLock<indexmap::IndexMap<&'static str, ObjectVtable>> = LazyLock::new(|| {
    let mut r = Registration {
        m: indexmap::IndexMap::with_capacity(REGISTERED_TYPES.len()),
    };
    for rt in REGISTERED_TYPES {
        rt(&mut r);
    }
    r.m
});

#[macro_export]
macro_rules! register_type {
    ($type:ty) => {
        paste::paste! {
            #[$crate::register($crate::types::object::REGISTERED_TYPES)]
            #[allow(non_upper_case_globals)]
            static [<REGISTER_CEL_$type>]: fn(&mut $crate::types::object::Registration) = [<register_cel_ $type>];
            #[allow(non_snake_case)]
            fn [<register_cel_ $type>](registration: &mut $crate::types::object::Registration) {
                registration.register::<$type>()
            }
        }
    };
}

fn vtable<T>() -> Option<&'static ObjectVtable> {
    VTABLES.get(&std::any::type_name::<T>())
}

pub struct Registration {
    m: indexmap::IndexMap<&'static str, ObjectVtable>,
}
impl Registration {
    pub fn register<'a, T: ObjectType<'a> + PartialEq + 'static>(&mut self) {
        let id = std::any::type_name::<T>();
        match self.m.entry(id) {
            Entry::Occupied(_) => panic!("type registered twice!"),
            Entry::Vacant(v) => {
                let vtable = make_vtable::<T>();
                v.insert(vtable);
            }
        }
    }
}

/// A covariant Arc-based container for user-defined object values.
///
/// This type stores values implementing [`ObjectType`] using Arc for cheap cloning.
/// It is covariant in 'a because:
/// - The actual data pointer doesn't carry lifetime info (it's erased)
/// - PhantomData<fn() -> Value<'a>> is covariant in 'a (function return types are covariant)
///
/// # Example
/// ```rust
/// use cel::objects::{ObjectValue, ObjectType, Value};
///
/// #[derive(Clone, Debug, PartialEq)]
/// struct MyStruct { field: String }
///
/// impl ObjectType<'static> for MyStruct {
///     fn type_name(&self) -> &'static str { "MyStruct" }
/// }
///
/// let obj = ObjectValue::new(MyStruct { field: "test".into() });
/// let value: Value = obj.into();
/// ```
pub struct ObjectValue<'a> {
    // Type-erased Arc pointer - we store the raw pointer from Arc::into_raw()
    // and manually manage the reference count via the vtable
    data: NonNull<()>,
    vtable: &'static ObjectVtable,
    // fn() -> T is covariant in T, so fn() -> Value<'a> is covariant in 'a
    _marker: PhantomData<fn() -> Value<'a>>,
}

// Safety: Object is Send/Sync because the underlying T: Send + Sync (enforced by ObjectValue bound)
unsafe impl<'a> Send for ObjectValue<'a> {}
unsafe impl<'a> Sync for ObjectValue<'a> {}

impl<'a> ObjectValue<'a> {
    /// Create a new Object from a value implementing ObjectValue
    pub fn new<T>(value: T) -> Self
    where
        T: ObjectType<'a> + PartialEq + 'a,
    {
        let arc = Arc::new(value);
        let ptr = NonNull::new(Arc::into_raw(arc) as *mut ()).unwrap();

        // Get or create the vtable for type T from the global registry
        let vtable = vtable::<T>().unwrap_or_else(|| {
            panic!(
                "type {} must be registered before usage",
                std::any::type_name::<T>()
            )
        });
        ObjectValue {
            data: ptr,
            vtable,
            _marker: PhantomData,
        }
    }

    /// Returns the type name of the contained value.
    pub fn type_name(&self) -> &'static str {
        unsafe { (self.vtable.type_name)(self.data) }
    }

    /// Returns a member/field value by name.
    pub fn get_member(&self, name: &str) -> Option<Value<'a>> {
        unsafe {
            // Safety: vtable.get_member returns Value<'static> but we cast to Value<'a>
            // This is sound because the actual lifetime is 'a (enforced by PhantomData)
            std::mem::transmute((self.vtable.get_member)(self.data, name))
        }
    }

    /// Resolves a method function by name.
    pub fn resolve_function(&self, name: &str) -> Option<&Function> {
        unsafe {
            // Safety: similar to get_member
            std::mem::transmute((self.vtable.resolve_function)(self.data, name))
        }
    }

    /// Attempts to downcast to a concrete type.
    pub fn downcast_ref<T>(&self) -> Option<&T> {
        let tname = std::any::type_name::<T>();
        if tname == self.vtable.rust_type_name {
            // Safety: We verified the type matches using the Rust type name
            Some(unsafe { &*(self.data.as_ptr() as *const T) })
        } else {
            None
        }
    }

    /// Returns the JSON representation if available.
    #[cfg(feature = "json")]
    pub fn json(&self) -> Option<serde_json::Value> {
        unsafe { (self.vtable.json)(self.data) }
    }
}

impl PartialEq for ObjectValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        // First check if the types match
        let self_type = unsafe { (self.vtable.type_name)(self.data) };
        let other_type = unsafe { (other.vtable.type_name)(other.data) };
        if self_type != other_type {
            return false;
        }
        // Then compare the values using the vtable's eq function
        unsafe { (self.vtable.eq)(self.data, other.data) }
    }
}

impl<'a> std::fmt::Debug for ObjectValue<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe { (self.vtable.debug)(self.data, f) }
    }
}

impl<'a> Drop for ObjectValue<'a> {
    fn drop(&mut self) {
        unsafe { (self.vtable.drop)(self.data) }
    }
}

impl<'a> Clone for ObjectValue<'a> {
    fn clone(&self) -> Self {
        ObjectValue {
            data: unsafe { (self.vtable.clone)(self.data) },
            vtable: self.vtable,
            _marker: PhantomData,
        }
    }
}

/// A custom vtable for Object values
struct ObjectVtable {
    type_name: unsafe fn(NonNull<()>) -> &'static str,
    rust_type_name: &'static str, // For downcast comparison
    get_member: unsafe fn(NonNull<()>, &str) -> Option<Value<'static>>,
    resolve_function: unsafe fn(NonNull<()>, &str) -> Option<&Function>,
    #[cfg(feature = "json")]
    json: unsafe fn(NonNull<()>) -> Option<serde_json::Value>,
    debug: unsafe fn(NonNull<()>, &mut std::fmt::Formatter<'_>) -> std::fmt::Result,
    drop: unsafe fn(NonNull<()>),
    clone: unsafe fn(NonNull<()>) -> NonNull<()>,
    eq: unsafe fn(NonNull<()>, NonNull<()>) -> bool,
}

fn make_vtable<'a, T: ObjectType<'a> + PartialEq + 'a>() -> ObjectVtable {
    // These functions are safe because we only call them with the correct type T
    unsafe fn get_member_impl<'a, T: ObjectType<'a>>(
        ptr: NonNull<()>,
        name: &str,
    ) -> Option<Value<'static>> {
        unsafe {
            let value = &*(ptr.as_ptr() as *const T);
            // Safety: We're transmuting Value<'a> to Value<'static>
            // This is safe because:
            // 1. The caller (Object::get_member) will immediately cast it back to Value<'a>
            // 2. The Object's PhantomData ensures the correct lifetime is tracked
            std::mem::transmute(value.get_member(name))
        }
    }

    unsafe fn resolve_function_impl<'a, T: ObjectType<'a>>(
        ptr: NonNull<()>,
        name: &str,
    ) -> Option<&Function> {
        unsafe {
            let value = &*(ptr.as_ptr() as *const T);
            std::mem::transmute(value.resolve_function(name))
        }
    }

    #[cfg(feature = "json")]
    unsafe fn json_impl<'a, T: ObjectType<'a>>(ptr: NonNull<()>) -> Option<serde_json::Value> {
        unsafe {
            let value = &*(ptr.as_ptr() as *const T);
            value.json()
        }
    }

    unsafe fn debug_impl<'a, T: ObjectType<'a>>(
        ptr: NonNull<()>,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        unsafe {
            let value = &*(ptr.as_ptr() as *const T);
            std::fmt::Debug::fmt(value, f)
        }
    }

    unsafe fn drop_impl<T>(ptr: NonNull<()>) {
        unsafe {
            // Decrement the Arc's strong count; if it reaches zero, the value is dropped
            Arc::decrement_strong_count(ptr.as_ptr() as *const T);
        }
    }

    unsafe fn clone_impl<T>(ptr: NonNull<()>) -> NonNull<()> {
        unsafe {
            // Increment the Arc's strong count and return the same pointer
            Arc::increment_strong_count(ptr.as_ptr() as *const T);
            ptr
        }
    }

    unsafe fn type_name_impl<'a, T: ObjectType<'a>>(ptr: NonNull<()>) -> &'static str {
        unsafe {
            let value = &*(ptr.as_ptr() as *const T);
            value.type_name()
        }
    }

    unsafe fn eq_impl<T: PartialEq>(a: NonNull<()>, b: NonNull<()>) -> bool {
        unsafe {
            let a = &*(a.as_ptr() as *const T);
            let b = &*(b.as_ptr() as *const T);
            a == b
        }
    }

    // Create the vtable (one per type T, cached via OnceLock)
    unsafe {
        #[allow(clippy::missing_transmute_annotations)]
        ObjectVtable {
            type_name: type_name_impl::<T>,
            rust_type_name: std::any::type_name::<T>(),
            get_member: std::mem::transmute(
                get_member_impl::<T> as unsafe fn(NonNull<()>, &str) -> Option<Value<'a>>,
            ),
            resolve_function: std::mem::transmute(
                resolve_function_impl::<T> as unsafe fn(NonNull<()>, &str) -> Option<&Function>,
            ),
            #[cfg(feature = "json")]
            json: json_impl::<T>,
            debug: std::mem::transmute(
                debug_impl::<T>
                    as unsafe fn(NonNull<()>, &mut std::fmt::Formatter<'_>) -> std::fmt::Result,
            ),
            drop: drop_impl::<T>,
            clone: clone_impl::<T>,
            eq: eq_impl::<T>,
        }
    }
}

use cel::Value;

/// A boxed value representation for passing CEL values through compiled code.
///
/// Uses tagged pointers with the low 3 bits as tags:
/// - `0b000`: Pointer to heap-allocated `Value`
/// - `0b001`: Inline small integer (value shifted left 3 bits)
/// - `0b010`: Inline boolean (0 or 1 in upper bits)
/// - `0b011`: Null
#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
pub struct BoxedValue(pub u64);

impl BoxedValue {
    // Tag constants - using low 3 bits
    const TAG_MASK: u64 = 0b111;
    const TAG_PTR: u64 = 0b000;
    const TAG_SMALL_INT: u64 = 0b001;
    const TAG_BOOL: u64 = 0b010;
    const TAG_NULL: u64 = 0b011;

    // Small int range: we can fit 61 bits of integer
    const SMALL_INT_MIN: i64 = -(1 << 60);
    const SMALL_INT_MAX: i64 = (1 << 60) - 1;

    /// Create a BoxedValue from a CEL Value.
    pub fn from_value(val: Value) -> Self {
        match &val {
            Value::Int(i) if *i >= Self::SMALL_INT_MIN && *i <= Self::SMALL_INT_MAX => {
                // Inline small integer
                BoxedValue(((*i as u64) << 3) | Self::TAG_SMALL_INT)
            }
            Value::Bool(b) => {
                // Inline boolean
                BoxedValue(((*b as u64) << 3) | Self::TAG_BOOL)
            }
            Value::Null => BoxedValue(Self::TAG_NULL),
            _ => {
                // Heap allocate
                let boxed = Box::new(val);
                let ptr = Box::into_raw(boxed) as u64;
                // Ensure pointer is properly aligned (8-byte alignment means low 3 bits are 0)
                debug_assert!(ptr & Self::TAG_MASK == 0, "Pointer not properly aligned");
                BoxedValue(ptr | Self::TAG_PTR)
            }
        }
    }

    /// Convert back to a CEL Value.
    /// This clones heap-allocated values. For zero-copy access, use `as_value_ref()`.
    ///
    /// # Safety
    /// If the value contains a pointer, it must be a valid pointer to a Value.
    pub fn to_value(self) -> Value {
        match self.0 & Self::TAG_MASK {
            Self::TAG_PTR => {
                if self.0 == 0 {
                    // Null pointer - treat as Null
                    Value::Null
                } else {
                    let ptr = (self.0 & !Self::TAG_MASK) as *const Value;
                    // Clone the value (don't consume the box)
                    unsafe { (*ptr).clone() }
                }
            }
            Self::TAG_SMALL_INT => {
                // Sign-extend the shifted value
                let shifted = self.0 >> 3;
                let val = (shifted as i64) | (if shifted & (1 << 60) != 0 { !0 << 61 } else { 0 });
                Value::Int(val)
            }
            Self::TAG_BOOL => Value::Bool((self.0 >> 3) != 0),
            Self::TAG_NULL => Value::Null,
            _ => unreachable!("Invalid tag"),
        }
    }

    /// Get a reference to the underlying Value without cloning.
    /// Returns None for inline values (small int, bool, null) which don't have a heap reference.
    ///
    /// # Safety
    /// If the value contains a pointer, it must be a valid pointer to a Value.
    #[inline]
    pub unsafe fn as_value_ref(&self) -> Option<&Value> {
        if self.tag() == Self::TAG_PTR && self.0 != 0 {
            let ptr = (self.0 & !Self::TAG_MASK) as *const Value;
            Some(&*ptr)
        } else {
            None
        }
    }

    /// Try to extract inline integer without heap access.
    #[inline]
    pub fn try_as_int(&self) -> Option<i64> {
        if self.tag() == Self::TAG_SMALL_INT {
            let shifted = self.0 >> 3;
            let val = (shifted as i64) | (if shifted & (1 << 60) != 0 { !0 << 61 } else { 0 });
            Some(val)
        } else if self.tag() == Self::TAG_PTR && self.0 != 0 {
            let ptr = (self.0 & !Self::TAG_MASK) as *const Value;
            match unsafe { &*ptr } {
                Value::Int(i) => Some(*i),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Try to extract inline boolean without heap access.
    #[inline]
    pub fn try_as_bool(&self) -> Option<bool> {
        if self.tag() == Self::TAG_BOOL {
            Some((self.0 >> 3) != 0)
        } else if self.tag() == Self::TAG_PTR && self.0 != 0 {
            let ptr = (self.0 & !Self::TAG_MASK) as *const Value;
            match unsafe { &*ptr } {
                Value::Bool(b) => Some(*b),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Consume and convert to Value, freeing any heap allocation.
    ///
    /// # Safety
    /// If the value contains a pointer, it must be a valid pointer to a Value,
    /// and must not be used after this call.
    pub unsafe fn into_value(self) -> Value {
        match self.0 & Self::TAG_MASK {
            Self::TAG_PTR => {
                if self.0 == 0 {
                    Value::Null
                } else {
                    let ptr = (self.0 & !Self::TAG_MASK) as *mut Value;
                    *Box::from_raw(ptr)
                }
            }
            _ => self.to_value(),
        }
    }

    /// Get the tag of this value.
    #[inline]
    pub fn tag(self) -> u64 {
        self.0 & Self::TAG_MASK
    }

    /// Check if this is a null value.
    #[inline]
    pub fn is_null(self) -> bool {
        self.0 == Self::TAG_NULL
    }

    /// Check if this is an inline small integer.
    #[inline]
    pub fn is_small_int(self) -> bool {
        self.tag() == Self::TAG_SMALL_INT
    }

    /// Check if this is an inline boolean.
    #[inline]
    pub fn is_bool(self) -> bool {
        self.tag() == Self::TAG_BOOL
    }

    /// Check if this is a heap-allocated pointer.
    #[inline]
    pub fn is_ptr(self) -> bool {
        self.tag() == Self::TAG_PTR && self.0 != 0
    }

    /// Create a null BoxedValue.
    #[inline]
    pub const fn null() -> Self {
        BoxedValue(Self::TAG_NULL)
    }

    /// Create a boolean BoxedValue.
    #[inline]
    pub const fn bool(b: bool) -> Self {
        BoxedValue(((b as u64) << 3) | Self::TAG_BOOL)
    }

    /// Create a small integer BoxedValue.
    /// Returns None if the value is out of range.
    #[inline]
    pub fn small_int(i: i64) -> Option<Self> {
        if (Self::SMALL_INT_MIN..=Self::SMALL_INT_MAX).contains(&i) {
            Some(BoxedValue(((i as u64) << 3) | Self::TAG_SMALL_INT))
        } else {
            None
        }
    }

    /// Get as raw u64 for passing through compiled code.
    #[inline]
    pub fn as_raw(self) -> u64 {
        self.0
    }

    /// Create from raw u64 received from compiled code.
    ///
    /// # Safety
    /// The raw value must be a valid BoxedValue representation.
    #[inline]
    pub unsafe fn from_raw(raw: u64) -> Self {
        BoxedValue(raw)
    }
}

impl From<Value> for BoxedValue {
    fn from(val: Value) -> Self {
        BoxedValue::from_value(val)
    }
}

impl Default for BoxedValue {
    fn default() -> Self {
        BoxedValue::null()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null() {
        let boxed = BoxedValue::null();
        assert!(boxed.is_null());
        assert_eq!(boxed.to_value(), Value::Null);
    }

    #[test]
    fn test_bool() {
        let boxed_true = BoxedValue::bool(true);
        let boxed_false = BoxedValue::bool(false);

        assert!(boxed_true.is_bool());
        assert!(boxed_false.is_bool());

        assert_eq!(boxed_true.to_value(), Value::Bool(true));
        assert_eq!(boxed_false.to_value(), Value::Bool(false));
    }

    #[test]
    fn test_small_int() {
        let test_values = [0i64, 1, -1, 100, -100, 1000000, -1000000];

        for val in test_values {
            let boxed = BoxedValue::small_int(val).unwrap();
            assert!(boxed.is_small_int());
            assert_eq!(boxed.to_value(), Value::Int(val));
        }
    }

    #[test]
    fn test_heap_string() {
        use std::sync::Arc;
        let val = Value::String(Arc::new("hello world".to_string()));
        let boxed = BoxedValue::from_value(val.clone());

        assert!(boxed.is_ptr());
        assert_eq!(boxed.to_value(), val);

        // Clean up - consume the boxed value
        unsafe {
            let _ = boxed.into_value();
        }
    }

    #[test]
    fn test_roundtrip() {
        use std::sync::Arc;

        let values = vec![
            Value::Null,
            Value::Bool(true),
            Value::Bool(false),
            Value::Int(0),
            Value::Int(42),
            Value::Int(-42),
            Value::UInt(100),
            Value::Float(3.14),
            Value::String(Arc::new("test".to_string())),
        ];

        for val in values {
            let boxed = BoxedValue::from_value(val.clone());
            let recovered = boxed.to_value();
            assert_eq!(recovered, val, "Roundtrip failed for {:?}", val);

            // Clean up heap values
            if boxed.is_ptr() {
                unsafe {
                    let _ = BoxedValue(boxed.0).into_value();
                }
            }
        }
    }
}

use crate::common::{types::Type, value::Val};

#[derive(Debug, PartialEq, Eq)]
pub struct Struct {
    r#type: Type<'static>,
}

impl Struct {
    pub fn new(name: String) -> Self {
        Self {
            r#type: Type::new_struct_type(name.leak()),
        }
    }

    pub fn name(&self) -> &str {
        self.r#type.name()
    }
}

impl Val for Struct {
    fn get_type(&self) -> &Type<'_> {
        &self.r#type
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Self {
            r#type: Type::new_struct_type(self.name().to_owned().leak()),
        })
    }
}

impl Drop for Struct {
    fn drop(&mut self) {
        let name = self.r#type.name();

        let ptr = name.as_ptr();
        let len = name.len();

        // SAFETY `Type` is not `Clone` and as such solely owned by this `Struct` being dropped
        // We leak the name on `Struct::new` to get a &'static str, that we now no longer need
        let name = unsafe { String::from_raw_parts(ptr as *mut u8, len, len) };
        std::mem::drop(name);
    }
}

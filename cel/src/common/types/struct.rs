use std::{borrow::Cow, collections::BTreeMap, ops::Deref, sync::Arc};

use crate::common::{types::Type, value::Val};

#[derive(Debug)]
pub struct Struct {
    r#type: Type<'static>,
    entries: BTreeMap<String, Arc<dyn Val>>,
}

impl Struct {
    pub fn new(name: String) -> Self {
        Self {
            r#type: Type::new_struct_type(name.leak()),
            entries: BTreeMap::default(),
        }
    }

    pub fn name(&self) -> &str {
        self.r#type.name()
    }

    pub fn field_value(&self, name: &str) -> Option<&dyn Val> {
        self.entries.get(name).map(Deref::deref)
    }

    pub fn add_field_value(&mut self, name: String, value: Cow<dyn Val>) {
        self.entries.insert(name, Arc::from(value.into_owned()));
    }

    pub fn field_values(&self) -> BTreeMap<String, Arc<dyn Val>> {
        self.entries.clone()
    }
}

impl Val for Struct {
    fn get_type(&self) -> &Type<'_> {
        &self.r#type
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Self {
            r#type: Type::new_struct_type(self.name().to_owned().leak()),
            entries: self
                .entries
                .iter()
                .map(|(k, v)| (k.clone(), Arc::from(v.clone_as_boxed())))
                .collect(),
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

use crate::common::{
    decls::FunctionDecl,
    functions::Function,
    types::{self, Type},
    value::Val,
};
#[cfg(feature = "structs")]
use crate::{common::types::CelStruct, ExecutionError};
use std::{
    borrow::Cow,
    collections::{
        btree_map::Entry::{Occupied, Vacant},
        BTreeMap,
    },
};

#[derive(Default)]
pub struct Env {
    functions: BTreeMap<String, FunctionDecl>,
    #[cfg(feature = "structs")]
    structs: BTreeMap<String, StructDef>,
}

impl Env {
    pub fn stdlib() -> Env {
        let mut env = Env::default();
        types::bytes::stdlib(&mut env);
        types::double::stdlib(&mut env);
        types::int::stdlib(&mut env);
        types::list::stdlib(&mut env);
        types::map::stdlib(&mut env);
        types::optional::stdlib(&mut env);
        types::string::stdlib(&mut env);
        types::uint::stdlib(&mut env);

        #[cfg(feature = "chrono")]
        {
            types::duration::stdlib(&mut env);
            types::timestamp::stdlib(&mut env);
        }
        env
    }

    #[allow(clippy::result_unit_err)]
    pub fn add_overload(
        &mut self,
        name: &str,
        id: &str,
        args: Vec<types::Type>,
        op: Function,
    ) -> Result<(), ()> {
        match self.functions.entry(name.to_owned()) {
            Vacant(vacant_entry) => {
                let mut value = FunctionDecl::new(name);
                value.add_overload(id.to_string(), false, args, op)?;
                vacant_entry.insert(value);
                Ok(())
            }
            Occupied(occupied_entry) => {
                occupied_entry
                    .into_mut()
                    .add_overload(id.to_string(), false, args, op)
            }
        }
    }

    pub fn find_overload(&self, name: &str, args: &[Cow<dyn Val>]) -> Option<Function> {
        match self.functions.get(name) {
            None => None,
            Some(fn_decl) => fn_decl.find_overload(false, args),
        }
    }

    #[allow(clippy::result_unit_err)]
    pub fn add_member_overload(
        &mut self,
        name: &str,
        id: &str,
        target: Type,
        args: Vec<types::Type>,
        op: Function,
    ) -> Result<(), ()> {
        let mut args = args;
        args.insert(0, target);
        match self.functions.entry(name.to_owned()) {
            Vacant(vacant_entry) => {
                let mut value = FunctionDecl::new(name);
                value.add_overload(id.to_string(), true, args, op)?;
                vacant_entry.insert(value);
                Ok(())
            }
            Occupied(occupied_entry) => {
                occupied_entry
                    .into_mut()
                    .add_overload(id.to_string(), true, args, op)
            }
        }
    }

    pub fn find_member_overload(&self, name: &str, args: &[Cow<dyn Val>]) -> Option<Function> {
        match self.functions.get(name) {
            None => None,
            Some(fn_decl) => fn_decl.find_overload(true, args),
        }
    }

    #[cfg(feature = "structs")]
    pub fn add_struct(&mut self, def: StructDef) {
        self.structs.insert(def.name.clone(), def);
    }

    #[cfg(feature = "structs")]
    pub(crate) fn find_struct(&self, name: &str) -> Option<&StructDef> {
        self.structs.get(name)
    }
}

#[cfg(feature = "structs")]
pub struct StructDef {
    name: String,
    fields: BTreeMap<String, Type>,
    defaults: BTreeMap<String, Box<dyn Val>>,
}

#[cfg(feature = "structs")]
impl StructDef {
    pub fn new(name: String) -> Self {
        Self {
            name,
            fields: Default::default(),
            defaults: Default::default(),
        }
    }

    pub fn add_field(self, field: String, t: Type) -> Self {
        self.insert_field(field, t, None)
    }

    pub fn add_field_with_default(self, field: String, default: Box<dyn Val>) -> Self {
        self.insert_field(field, default.get_type().to_owned(), Some(default))
    }

    fn insert_field(self, field: String, t: Type, default: Option<Box<dyn Val>>) -> Self {
        let mut def = self;
        def.fields.insert(field.clone(), t);
        if let Some(default) = default {
            def.defaults.insert(field, default);
        }
        def
    }

    pub(crate) fn new_struct(
        &self,
        fields: BTreeMap<String, std::borrow::Cow<dyn Val>>,
    ) -> Result<CelStruct, ExecutionError> {
        let mut s = CelStruct::new(self.name.clone());
        let mut fields = fields;
        for (field, default) in &self.defaults {
            if let Some(value) = fields.remove(field) {
                s.add_field_value(field.clone(), value);
            } else {
                s.add_field_value(field.clone(), Cow::Owned(default.clone_as_boxed()));
            }
        }
        for (field, value) in fields {
            match self.fields.get(&field) {
                Some(t) => {
                    if t != value.get_type() {
                        return Err(ExecutionError::UnexpectedType {
                            got: value.get_type().name().to_owned(),
                            want: format!("{} for field {field} in {}", t.name(), self.name),
                        });
                    }
                    s.add_field_value(field, value);
                }
                None => {
                    return Err(ExecutionError::NoSuchKey(std::sync::Arc::new(format!(
                        "field `{field}` on struct `{}`",
                        self.name
                    ))))
                }
            }
        }
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_env_default() {
        let _: Arc<dyn Send + Sync> = Arc::new(Env::default());
    }
}

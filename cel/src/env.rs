use crate::common::{
    decls::FunctionDecl,
    functions::Function,
    types::{self, Type},
    value::Val,
};
use std::{
    borrow::Cow,
    collections::{
        btree_map::Entry::{Occupied, Vacant},
        BTreeMap,
    },
};

#[derive(Default)]
pub struct Env<'a> {
    functions: BTreeMap<String, FunctionDecl<'a>>,
}

impl<'a> Env<'a> {
    pub fn stdlib() -> Env<'a> {
        let mut env = Env::default();
        types::bytes::stdlib(&mut env);
        types::double::stdlib(&mut env);
        types::int::stdlib(&mut env);
        types::list::stdlib(&mut env);
        types::map::stdlib(&mut env);
        types::string::stdlib(&mut env);

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
        args: Vec<types::Type<'a>>,
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
        target: Type<'a>,
        args: Vec<types::Type<'a>>,
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

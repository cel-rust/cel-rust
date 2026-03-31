use crate::common::{
    decls::FunctionDecl,
    functions::Function,
    types::{self, Type},
};
use std::collections::{
    btree_map::Entry::{Occupied, Vacant},
    BTreeMap,
};

#[derive(Default)]
pub struct Env<'a> {
    functions: BTreeMap<String, FunctionDecl<'a>>,
}

impl<'a> Env<'a> {
    pub fn stdlib() -> Env<'a> {
        let mut env = Env::default();
        env.add_overload(
            "size",
            "size_bytes",
            vec![types::BYTES_TYPE],
            types::bytes::size_fn,
        ).expect("Must be unique id");
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

    pub fn find_overload(&self, name: &str, args: &[Type<'_>]) -> Option<Function> {
        match self.functions.get(name) {
            None => None,
            Some(fn_decl) => fn_decl.find_overload(false, args),
        }
    }

    pub fn find_member_overload(
        &self,
        _name: &str,
        _target: Type<'_>,
        _args: &[Type<'_>],
    ) -> Option<Function> {
        None
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

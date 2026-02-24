use crate::common::decls::FunctionDecl;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Env<'a> {
    functions: BTreeMap<String, FunctionDecl<'a>>,
}

impl<'a> Env<'a> {
    pub fn stdlib() -> Env<'a> {
        Env::default()
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

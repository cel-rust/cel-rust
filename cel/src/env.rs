use crate::common::ast::Ast;
use crate::common::functions::UnaryOp;
use crate::common::types;
use crate::common::types::Type;
use crate::magic::IntoFunction;

pub struct Env {}

impl Env {
    pub fn builder() -> EnvBuilder {
        EnvBuilder::default()
    }

    pub fn parse(&self, text: &str) -> Result<Ast, ()> {
        todo!("Implement this!")
    }

    pub fn check(&self, ast: Ast) -> Result<Ast, ()> {
        Ok(ast)
    }

    pub fn compile(&self, text: &str) -> Result<Ast, ()> {
        self.check(self.parse(text)?)
    }
}

pub struct EnvBuilder {}

impl EnvBuilder {
    pub fn build(&self) -> Env {
        Env {}
    }

    pub fn add_type(&mut self, t: Type) -> &mut Self {
        self
    }

    pub fn add_variable(&mut self, binding: &str, t: Type) -> &mut Self {
        self
    }

    pub fn add_overload<T: 'static, F>(
        &mut self,
        fn_name: &str,
        overload_name: &str,
        arg_types: &[&Type],
        ret_type: &Type,
        f: F
    ) -> &mut Self
    where
        F: IntoFunction<T> + 'static + Send + Sync,
    {
        self
    }
    
    pub fn add_member_overload<T: 'static, F>(
        &mut self,
        fn_name: &str,
        overload_name: &str,
        target_type: &Type,
        arg_types: &[&Type],
        ret_type: &Type,
        f: F
    ) -> &mut Self
    where
        F: IntoFunction<T> + 'static + Send + Sync,
    {
        self
    }
}

impl Default for EnvBuilder {
    fn default() -> Self {
        Self {}
    }
}

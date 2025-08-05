use crate::common::ast::Ast;
use crate::common::types::Type;
use crate::magic::IntoFunction;

pub struct Env {}

impl Env {
    pub fn builder() -> EnvBuilder {
        EnvBuilder::default()
    }

    pub fn parse(&self, _text: &str) -> Result<Ast, ()> {
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

    pub fn add_type(&mut self, _t: Type) -> &mut Self {
        self
    }

    /// Adds [`t`], if unknown or added with [add_type] prior.
    /// Should this be called `variable`, what if the [`binding`] already is declared?
    pub fn add_variable(&mut self, _binding: &str, _t: &Type) -> &mut Self {
        self
    }

    /// Same as [`add_variable`], which includes collisions for [`overload_name`]?
    pub fn add_overload<T: 'static, F>(
        &mut self,
        _fn_name: &str,
        _overload_name: &str,
        _arg_types: &[&Type],
        _ret_type: &Type,
        _f: F,
    ) -> &mut Self
    where
        F: IntoFunction<T> + 'static + Send + Sync,
    {
        self
    }

    /// Same as [`add_overload`], including issues
    pub fn add_member_overload<T: 'static, F>(
        &mut self,
        _fn_name: &str,
        _overload_name: &str,
        _target_type: &Type,
        _arg_types: &[&Type],
        _ret_type: &Type,
        _f: F,
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

#[cfg(test)]
mod tests {
    use crate::common::ast::operators;
    use crate::common::traits::Adder;
    use crate::common::types;
    use crate::common::value::Val;
    use crate::Env;

    #[test]
    fn api() {
        let _env = Env::builder()
            .add_overload(
                operators::ADD,
                "add_int64",
                &[&types::INT_TYPE, &types::INT_TYPE],
                &types::INT_TYPE,
                add_int64,
            )
            .build();

        // let v = CelVal::Int(16);
        // let v = v.into_val();
        // let t = v.add(v.as_ref());
        //assert_eq!(32i64, t.deref().downcast_ref::<i64>().unwrap().clone());

        let int: types::Int = 16.into();
        let val: &dyn Val = &int;
        if let Some(i) = val.downcast_ref::<types::Int>() {
            println!("{:?}", i.add(&int));
        }

        //let total = val.as_trait::<traits::Adder>().unwrap().add(&int);

        // assert_eq!(Box::new(32i64), adder.add(CelVal::Int(16i64).into_val().deref()).into_inner().downcast::<i64>().unwrap());
    }

    fn add_int64(lhs: i64, rhs: i64) -> i64 {
        lhs + rhs
    }
}

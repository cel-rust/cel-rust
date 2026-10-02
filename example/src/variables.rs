use cel::{Context, Env};
use std::sync::Arc;

fn main() {
    let env = Arc::new(Env::stdlib());
    let program = env.compile("foo * 2").unwrap();
    let mut context = Context::with_env(env);
    context.add_variable("foo", 10).unwrap();

    let value = program.execute(&context).unwrap();
    assert_eq!(value, 20.into());
}

use cel::{Context, Env};
use std::sync::Arc;

fn main() {
    let env = Arc::new(Env::stdlib());
    let program = env.compile("1 == 1").unwrap();
    let context = Context::with_env(env);
    let value = program.execute(&context).unwrap();
    assert_eq!(value, true.into());
}

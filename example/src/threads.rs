use cel::{Context, Env};
use std::sync::Arc;
use std::thread::scope;

fn main() {
    // One environment, shared by the program's compilation and every
    // context it's executed with.
    let env = Arc::new(Env::stdlib());
    let program = env.compile("a + b").unwrap();

    scope(|scope| {
        scope.spawn(|| {
            let mut context = Context::with_env(Arc::clone(&env));
            context.add_variable("a", 1).unwrap();
            context.add_variable("b", 2).unwrap();
            let value = program.execute(&context).unwrap();
            assert_eq!(value, 3.into());
        });
        scope.spawn(|| {
            let mut context = Context::with_env(Arc::clone(&env));
            context.add_variable("a", 2).unwrap();
            context.add_variable("b", 4).unwrap();
            let value = program.execute(&context).unwrap();
            assert_eq!(value, 6.into());
        });
    });
}

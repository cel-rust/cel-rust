use std::thread::scope;

use cel::{Context, Program};

fn main() {
    let program = Program::compile("a + b").unwrap();

    scope(|scope| {
        scope.spawn(|| {
            let mut context = Context::default();
            context.add_variable("a", 1).unwrap();
            context.add_variable("b", 2).unwrap();
            let value = program.execute(&context).unwrap();
            assert_eq!(value, 3.into());
        });
        scope.spawn(|| {
            let mut context = Context::default();
            context.add_variable("a", 2).unwrap();
            context.add_variable("b", 4).unwrap();
            let value = program.execute(&context).unwrap();
            assert_eq!(value, 6.into());
        });
    });
}

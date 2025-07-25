use cel::{Context, Program};

fn main() {
    let program = Program::compile("foo * 2").unwrap();
    let mut context = Context::default();
    context.add_variable("foo", 10).unwrap();

    let value = program.execute(&context).unwrap();
    assert_eq!(value, 20.into());
}

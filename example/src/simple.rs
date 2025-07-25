use cel::{Context, Program};

fn main() {
    let program = Program::compile("1 == 1").unwrap();
    let context = Context::default();
    let value = program.execute(&context).unwrap();
    assert_eq!(value, true.into());
}

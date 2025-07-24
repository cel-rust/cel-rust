use cel::{Context, Program};

fn main() {
    // Create a CEL program that returns a JSON object
    let program = Program::compile("{'foo': true}").unwrap();
    let value = program.execute(&Context::default()).unwrap();

    // Convert the return type to JSON and cast to object
    let json = value.json().unwrap();
    let object = json.as_object().unwrap();
    assert_eq!(Some(&serde_json::Value::Bool(true)), object.get("foo"));
}

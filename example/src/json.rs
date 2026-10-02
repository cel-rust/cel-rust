use cel::{Context, Env};
use std::sync::Arc;

fn main() {
    // Create a CEL program that returns a JSON object
    let env = Arc::new(Env::stdlib());
    let program = env.compile("{'foo': true}").unwrap();
    let value = program.execute(&Context::with_env(env)).unwrap();

    // Convert the return type to JSON and cast to object
    let json = value.json().unwrap();
    let object = json.as_object().unwrap();
    assert_eq!(Some(&serde_json::Value::Bool(true)), object.get("foo"));
}

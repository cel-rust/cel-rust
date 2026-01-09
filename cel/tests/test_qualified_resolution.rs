// Test for container-based fallback resolution of qualified identifiers.
//
// This test verifies that the new qualified identifier resolution correctly
// falls back to shorter container prefixes when longer ones don't match.
// This was NOT possible with the old Value::Namespace approach.
//
// With container "a.b.c" and a registered variable "a.b.X.Y":
// - Old code: Only tried "a.b.c.X.Y", failed with UndeclaredReference("X")
// - New code: Tries "a.b.c.X.Y", "a.b.X.Y" (found!), "a.X.Y", "X.Y" in order

use cel::{Context, Program, Value};

#[test]
fn test_container_fallback_for_qualified_names() {
    // With container "a.b.c", the identifier "X.Y" should try candidates in order:
    // 1. "a.b.c.X.Y" (not found)
    // 2. "a.b.X.Y" (found!)
    // 3. "a.X.Y"
    // 4. "X.Y"
    //
    // The old code would only try "a.b.c.X.Y" and fail.
    // The new code properly implements C++ namespace-style fallback resolution.
    let mut context = Context::default().with_container("a.b.c".to_string());
    context.add_variable("a.b.X.Y", Value::Int(888)).unwrap();

    let program = Program::compile("X.Y").unwrap();
    let value = program.execute(&context).unwrap();
    assert_eq!(value, Value::Int(888));
}

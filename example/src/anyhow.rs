use cel::Program;

/// This example demonstrates that compilation errors can be reported with anyhow.
fn main() {
    if let Err(e) = evaluate() {
        // Prints
        //
        // ERROR: <input>:1:3: Syntax error: token recognition error at: '@'
        // | 1 @ 1
        // | ..^
        // ERROR: <input>:1:5: Syntax error: extraneous input '1' expecting <EOF>
        // | 1 @ 1
        // | ....^
        eprintln!("{e}");
    }
}

fn evaluate() -> anyhow::Result<()> {
    Program::compile("1 @ 1")?;
    unreachable!()
}

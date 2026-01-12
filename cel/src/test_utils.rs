use crate::{Context, Program, ResolveResult, Value};

/// Tests the provided script and returns the result. An optional context can be provided.
pub(crate) fn test_script(script: &str, ctx: Option<Context>) -> ResolveResult {
    let program = match Program::compile(script) {
        Ok(program) => program,
        Err(err) => panic!("{err}"),
    };
    program.execute(&ctx.unwrap_or_default())
}

/// Asserts that the provided script evaluates to `true`.
pub(crate) fn assert_script(input: (&str, &str)) {
    let (description, script) = input;
    assert_eq!(test_script(script, None), Ok(true.into()), "{description}");
}

/// Asserts that the provided script evaluates and returns `expected`.
pub(crate) fn assert_script_eq(input: (&str, &str, Value)) {
    let (description, script, expected) = input;
    assert_eq!(test_script(script, None), Ok(expected), "{description}");
}

/// Asserts that the provided script returns an error and that the error matches
/// the provided message.
pub(crate) fn assert_error(input: (&str, &str, &str)) {
    let (description, script, expected_error_message) = input;
    let error_message = test_script(script, None)
        .expect_err("expected error")
        .to_string();
    assert_eq!(error_message, expected_error_message, "{description}",);
}

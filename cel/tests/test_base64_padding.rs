use cel::{Context, Env, ExecutionError, Program, Value};
use std::sync::Arc;

fn env_with_encoders() -> Env {
    let mut env = Env::stdlib();
    env.add_extension(cel::extensions::encoders)
        .expect("encoders extension is available");
    env
}

#[test]
fn base64_decode_accepts_full_or_no_padding() {
    let ctx = Context::with_env(Arc::new(env_with_encoders()));
    for expr in ["base64.decode('YQ==')", "base64.decode('YQ')"] {
        assert_eq!(
            Program::compile(expr)
                .expect("valid CEL")
                .execute(&ctx)
                .expect(expr),
            Value::from(b"a".to_vec()),
            "{expr} must decode"
        );
    }
}

#[test]
fn base64_decode_rejects_incomplete_padding() {
    let ctx = Context::with_env(Arc::new(env_with_encoders()));
    for expr in [
        "base64.decode('YQ=')",
        "base64.decode('aGVsbA=')",
        "base64.decode('aGVsbG8==')",
    ] {
        assert!(
            matches!(
                Program::compile(expr)
                    .expect("valid CEL")
                    .execute(&ctx),
                Err(ExecutionError::FunctionError { function, .. }) if function == "base64.decode"
            ),
            "{expr} must be rejected"
        );
    }
}

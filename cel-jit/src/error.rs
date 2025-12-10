use thiserror::Error;

#[derive(Error, Debug)]
pub enum CompileError {
    #[error("Cranelift error: {0}")]
    Cranelift(String),

    #[error("Module error: {0}")]
    Module(String),

    #[error("Parse error: {0}")]
    Parse(#[from] cel::ParseErrors),

    #[error("Unsupported expression: {0}")]
    UnsupportedExpression(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

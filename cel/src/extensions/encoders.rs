//! The encoders extension library: `base64.encode` and `base64.decode`,
//! following cel-go's `ext.Encoders()`.

use crate::common::types::{CelBytes, CelString};
use crate::{DeclarationError, Env, ExecutionError};
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use base64::prelude::*;

/// The standard alphabet, as lenient as Go's `base64.StdEncoding`, which
/// cel-go falls back to `RawStdEncoding` from: padding is optional, and
/// non-zero trailing bits are ignored.
const DECODER: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_padding_mode(DecodePaddingMode::Indifferent)
        .with_decode_allow_trailing_bits(true),
);

/// Registers the encoders extension's overloads on `env`.
pub fn extension(env: &mut Env) -> Result<(), DeclarationError> {
    crate::add_overload!(env, fn encode: (CelBytes) -> CelString, name = "base64.encode")?;
    crate::add_overload!(env, fn decode: (CelString) -> Result<CelBytes>, name = "base64.decode")?;
    Ok(())
}

fn encode(this: &CelBytes<'_>) -> CelString<'static> {
    BASE64_STANDARD.encode(this.inner()).into()
}

/// Decodes standard base64, skipping line breaks, as Go's decoder does.
fn decode(this: &CelString<'_>) -> Result<CelBytes<'static>, ExecutionError> {
    let decoded = if this.contains(['\r', '\n']) {
        DECODER.decode(this.replace(['\r', '\n'], ""))
    } else {
        DECODER.decode(this.inner())
    };
    decoded
        .map(CelBytes::from)
        .map_err(|e| ExecutionError::function_error("base64.decode", e))
}

#[cfg(test)]
mod tests {
    use crate::{Context, Env, ExecutionError, Program, Value};
    use std::sync::Arc;

    #[test]
    fn base64_encodes_bytes_and_decodes_padded_or_unpadded_strings() {
        let mut env = Env::stdlib();
        env.add_extension(crate::extensions::encoders)
            .expect("We can't test the extension, if we can't register it");
        let ctx = Context::with_env(Arc::new(env));
        let eval = |expr: &str| {
            Program::compile(expr)
                .expect("This must be valid CEL")
                .execute(&ctx)
        };

        assert_eq!(eval("base64.encode(b'hello')"), Ok(Value::from("aGVsbG8=")));
        assert_eq!(
            eval("base64.decode('aGVsbG8=')"),
            Ok(Value::from(b"hello".to_vec()))
        );
        assert_eq!(
            eval("base64.decode('aGVsbG8')"),
            Ok(Value::from(b"hello".to_vec()))
        );
    }

    /// The inputs cel-go's decoder accepts beyond padded or unpadded
    /// base64, and one it still rejects.
    #[test]
    fn base64_decodes_as_leniently_as_cel_go() {
        let mut env = Env::stdlib();
        env.add_extension(crate::extensions::encoders)
            .expect("We can't test the extension, if we can't register it");
        let ctx = Context::with_env(Arc::new(env));
        let eval = |expr: &str| {
            Program::compile(expr)
                .expect("This must be valid CEL")
                .execute(&ctx)
        };

        // non-zero trailing bits
        assert_eq!(
            eval("base64.decode('aGVsbG9=')"),
            Ok(Value::from(b"hello".to_vec()))
        );
        // line breaks
        assert_eq!(
            eval("base64.decode('aGVs\\r\\nbG8=')"),
            Ok(Value::from(b"hello".to_vec()))
        );
        // too much padding
        assert!(matches!(
            eval("base64.decode('aGVsbG8==')"),
            Err(ExecutionError::FunctionError { function, .. }) if function == "base64.decode"
        ));
    }
}

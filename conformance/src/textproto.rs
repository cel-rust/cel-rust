use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage};
use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

// Load the FileDescriptorSet generated at build time
lazy_static::lazy_static! {
    static ref DESCRIPTOR_POOL: DescriptorPool = {
        let descriptor_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/file_descriptor_set.bin"));
        DescriptorPool::decode(descriptor_bytes.as_ref())
            .expect("Failed to load descriptor pool")
    };
}

/// Find protoc's well-known types include directory
fn find_protoc_include() -> Option<String> {
    // Try common locations for protoc's include directory
    // Prioritize Homebrew on macOS as it's most common
    let common_paths = [
        "/opt/homebrew/include", // macOS Homebrew (most common)
        "/usr/local/include",
        "/usr/include",
        "/usr/local/opt/protobuf/include", // macOS Homebrew protobuf
    ];

    for path in &common_paths {
        let well_known = std::path::Path::new(path).join("google").join("protobuf");
        // Verify wrappers.proto exists (needed for Int32Value, etc.)
        if well_known.join("wrappers.proto").exists() {
            return Some(path.to_string());
        }
    }

    // Try to get it from protoc binary location (for Homebrew)
    if let Ok(protoc_path) = which::which("protoc") {
        if let Some(bin_dir) = protoc_path.parent() {
            // Homebrew structure: /opt/homebrew/bin/protoc -> /opt/homebrew/include
            if let Some(brew_prefix) = bin_dir.parent() {
                let possible_include = brew_prefix.join("include");
                let well_known = possible_include.join("google").join("protobuf");
                if well_known.join("wrappers.proto").exists() {
                    return Some(possible_include.to_string_lossy().to_string());
                }
            }
        }
    }

    None
}

/// Build a descriptor set that includes all necessary proto files
fn build_descriptor_set(
    proto_files: &[&str],
    include_paths: &[&str],
) -> Result<tempfile::NamedTempFile, TextprotoParseError> {
    let descriptor_file = tempfile::NamedTempFile::new()?;
    let descriptor_path = descriptor_file.path().to_str().unwrap();

    let mut protoc_cmd = Command::new("protoc");
    protoc_cmd
        .arg("--descriptor_set_out")
        .arg(descriptor_path)
        .arg("--include_imports");

    // Add well-known types include path
    if let Some(well_known_include) = find_protoc_include() {
        protoc_cmd.arg("-I").arg(&well_known_include);
    }

    for include in include_paths {
        protoc_cmd.arg("-I").arg(include);
    }

    for proto_file in proto_files {
        protoc_cmd.arg(proto_file);
    }

    let output = protoc_cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TextprotoParseError::ProtocError(format!(
            "Failed to build descriptor set: {}",
            stderr
        )));
    }

    Ok(descriptor_file)
}

/// Inject empty message extension fields into wire format.
/// Note: This function turned out to inject at the wrong level (SimpleTestFile instead of TestAllTypes).
/// The actual injection now happens in value_converter.rs::inject_empty_message_extensions()
/// during Any-to-CEL conversion. Keeping this function skeleton for now in case we need it later.
fn inject_empty_extensions(_dynamic_msg: &DynamicMessage, _buf: &mut Vec<u8>) {
    // No-op: See note above
}

/// Parse textproto using prost-reflect (supports Any messages with type URLs)
fn parse_with_prost_reflect<T: Message + Default>(
    text: &str,
    message_type: &str,
) -> Result<T, TextprotoParseError> {
    // Get the message descriptor from the pool
    let message_desc = DESCRIPTOR_POOL
        .get_message_by_name(message_type)
        .ok_or_else(|| {
            TextprotoParseError::DescriptorError(format!(
                "Message type not found: {}",
                message_type
            ))
        })?;

    // Parse text format into DynamicMessage
    let dynamic_msg = DynamicMessage::parse_text_format(message_desc, text)
        .map_err(|e| TextprotoParseError::TextFormatError(e.to_string()))?;

    // Encode DynamicMessage to binary
    let mut buf = Vec::new();
    dynamic_msg
        .encode(&mut buf)
        .map_err(|e| TextprotoParseError::EncodeError(e.to_string()))?;

    // Fix: Inject empty message extension fields that were omitted during encoding
    // This is needed because protobuf spec omits empty optional messages, but we need
    // to detect their presence for proto.hasExt()
    inject_empty_extensions(&dynamic_msg, &mut buf);

    // Decode binary into prost-generated type
    T::decode(&buf[..]).map_err(TextprotoParseError::Decode)
}

/// Parse textproto using protoc to convert to binary format, then parse with prost (fallback)
fn parse_with_protoc<T: Message + Default>(
    text: &str,
    message_type: &str,
    proto_files: &[&str],
    include_paths: &[&str],
) -> Result<T, TextprotoParseError> {
    // Write textproto to a temporary file
    let mut textproto_file = NamedTempFile::new()?;
    textproto_file.write_all(text.as_bytes())?;

    // Build descriptor set (this helps with Any message resolution)
    let _descriptor_set = build_descriptor_set(proto_files, include_paths)?;

    // Use protoc to convert textproto to binary
    let mut protoc_cmd = Command::new("protoc");
    protoc_cmd.arg("--encode").arg(message_type);

    // Add well-known types include path
    if let Some(well_known_include) = find_protoc_include() {
        protoc_cmd.arg("-I").arg(&well_known_include);
    }

    for include in include_paths {
        protoc_cmd.arg("-I").arg(include);
    }

    for proto_file in proto_files {
        protoc_cmd.arg(proto_file);
    }

    let output = protoc_cmd
        .stdin(std::process::Stdio::from(textproto_file.reopen()?))
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TextprotoParseError::ProtocError(format!(
            "protoc failed: {}",
            stderr
        )));
    }

    // Parse the binary output with prost
    let message = T::decode(&output.stdout[..])?;
    Ok(message)
}

/// Parse textproto to prost type (tries prost-reflect first, falls back to protoc)
pub fn parse_textproto_to_prost<T: Message + Default>(
    text: &str,
    message_type: &str,
    proto_files: &[&str],
    include_paths: &[&str],
) -> Result<T, TextprotoParseError> {
    // Try prost-reflect first (handles Any messages with type URLs)
    match parse_with_prost_reflect(text, message_type) {
        Ok(result) => return Ok(result),
        Err(e) => {
            // If prost-reflect fails, fall back to protoc for better error messages
            eprintln!("prost-reflect parse failed: {}, trying protoc fallback", e);
        }
    }

    // Fallback to protoc-based parsing
    parse_with_protoc(text, message_type, proto_files, include_paths)
}

#[derive(Debug, thiserror::Error)]
pub enum TextprotoParseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Protoc error: {0}")]
    ProtocError(String),
    #[error("Descriptor error: {0}")]
    DescriptorError(String),
    #[error("Text format parse error: {0}")]
    TextFormatError(String),
    #[error("Encode error: {0}")]
    EncodeError(String),
    #[error("Protobuf decode error: {0}")]
    Decode(#[from] prost::DecodeError),
}

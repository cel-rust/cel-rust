use prost::Message;
use prost_reflect::DynamicMessage;

pub(super) fn parse_textproto_to_prost<T: Message + Default>(
    text: &str,
    message_type: &str,
) -> Result<T, TextprotoParseError> {
    let message_descriptor = crate::proto::descriptor_pool()
        .get_message_by_name(message_type)
        .ok_or_else(|| {
            TextprotoParseError::DescriptorError(format!(
                "Message type not found: {}",
                message_type
            ))
        })?;

    let dynamic_message = DynamicMessage::parse_text_format(message_descriptor, text)
        .map_err(|error| TextprotoParseError::TextFormatError(error.to_string()))?;

    let mut buf = Vec::new();
    dynamic_message
        .encode(&mut buf)
        .map_err(|error| TextprotoParseError::EncodeError(error.to_string()))?;

    T::decode(&buf[..]).map_err(TextprotoParseError::Decode)
}

#[derive(Debug, thiserror::Error)]
pub(super) enum TextprotoParseError {
    #[error("Descriptor error: {0}")]
    DescriptorError(String),
    #[error("Text format parse error: {0}")]
    TextFormatError(String),
    #[error("Encode error: {0}")]
    EncodeError(String),
    #[error("Protobuf decode error: {0}")]
    Decode(#[from] prost::DecodeError),
}

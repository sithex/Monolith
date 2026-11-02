//! Error contract (SPEC §5.4). Messages are user-presentable and never carry file contents,
//! tokens, or full command lines.

use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    Invalid,
    Unsupported,
    Unavailable,
    Busy,
    Io,
    Timeout,
    Internal,
}

#[derive(Serialize, Clone, Debug, specta::Type, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{code:?}: {message}")]
pub struct IslandError {
    pub code: ErrorCode,
    pub message: String,
}

impl IslandError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Invalid, message)
    }
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unsupported, message)
    }
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unavailable, message)
    }
    pub fn io(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Io, message)
    }
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Timeout, message)
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_camel_case_code_and_message_only() {
        let value = serde_json::to_value(IslandError::unsupported("no")).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"code": "unsupported", "message": "no"})
        );
    }
}

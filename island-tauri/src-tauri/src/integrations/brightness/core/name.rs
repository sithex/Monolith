//! Display-name validation and D-Bus object-path construction.

use crate::integrations::brightness::core::error::NameError;

/// Longest accepted display name in bytes (D-Bus object-path element limit).
pub const MAX_DISPLAY_NAME_BYTES: usize = 255;

/// Object path prefix for per-display child objects.
pub const OBJECT_PATH_PREFIX: &str = "/org/kde/ScreenBrightness/";

/// A display name must be 1..=255 ASCII `[A-Za-z0-9_]` bytes (it becomes a
/// D-Bus object-path element).
pub fn validate_display_name(name: &str) -> Result<(), NameError> {
    let bytes = name.as_bytes();
    let valid = !bytes.is_empty()
        && bytes.len() <= MAX_DISPLAY_NAME_BYTES
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_');
    if valid {
        Ok(())
    } else {
        Err(NameError::Invalid)
    }
}

/// `/org/kde/ScreenBrightness/<name>` for a validated display name.
pub fn display_path(name: &str) -> Result<String, NameError> {
    validate_display_name(name)?;
    Ok(format!("{OBJECT_PATH_PREFIX}{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names_accepted() {
        for name in ["display0", "a", "A_z9", "_"] {
            assert_eq!(validate_display_name(name), Ok(()), "name: {name:?}");
        }
        let long = "a".repeat(255);
        assert_eq!(validate_display_name(&long), Ok(()));
    }

    #[test]
    fn invalid_names_rejected() {
        for name in ["", "a-b", "a/b", "a b", "a.b", "é", "-a", "a+b", "a@b"] {
            assert_eq!(
                validate_display_name(name),
                Err(NameError::Invalid),
                "name: {name:?}"
            );
        }
        let long = "a".repeat(256);
        assert_eq!(validate_display_name(&long), Err(NameError::Invalid));
    }

    #[test]
    fn path_construction() {
        assert_eq!(
            display_path("display0"),
            Ok("/org/kde/ScreenBrightness/display0".to_string())
        );
        assert_eq!(display_path(""), Err(NameError::Invalid));
        assert_eq!(display_path("a-b"), Err(NameError::Invalid));
    }
}

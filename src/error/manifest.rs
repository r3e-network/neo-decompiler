use std::io;
use std::str::Utf8Error;

use thiserror::Error;

/// Errors returned while parsing Neo manifest files.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ManifestError {
    /// Failed to read the manifest file contents.
    #[error("failed to read manifest: {0}")]
    Io(#[from] io::Error),

    /// Manifest content exceeded the configured maximum size.
    #[error("manifest size {size} exceeds maximum {max}")]
    FileTooLarge {
        /// Manifest size in bytes.
        size: u64,
        /// Maximum allowed size in bytes.
        max: u64,
    },

    /// The manifest JSON failed to parse.
    #[error("manifest json parse error: {0}")]
    Json(#[from] serde_json::Error),

    /// The manifest bytes were not valid UTF-8.
    #[error("manifest contains invalid utf-8: {source}")]
    InvalidUtf8 {
        /// Original UTF-8 decoding error.
        #[source]
        source: Utf8Error,
    },

    /// The manifest passed JSON parsing but failed strict semantic validation.
    #[error("manifest validation error: {message}")]
    Validation {
        /// Human-readable validation failure details.
        message: String,
    },
}

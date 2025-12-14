use std::io;

use thiserror::Error;

/// Errors returned while parsing Neo manifest files.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ManifestError {
    /// Failed to read the manifest file contents.
    #[error("failed to read manifest: {0}")]
    Io(#[from] io::Error),

    /// The manifest JSON failed to parse.
    #[error("manifest json parse error: {0}")]
    Json(#[from] serde_json::Error),

    /// The manifest bytes were not valid UTF-8.
    #[error("manifest contains invalid utf-8: {error}")]
    InvalidUtf8 {
        /// Stringified UTF-8 decoding error.
        error: String,
    },
}

use std::fs;
use std::io::Read;
use std::path::Path;

use crate::error::{ManifestError, Result};

use super::ContractManifest;

impl ContractManifest {
    /// Load a manifest from a reader containing UTF-8 JSON.
    pub fn from_reader<R: Read>(mut reader: R) -> Result<Self> {
        let mut buf = String::new();
        reader
            .read_to_string(&mut buf)
            .map_err(ManifestError::from)?;
        Self::from_json_str(&buf)
    }

    /// Load a manifest from a raw JSON string.
    pub fn from_json_str(input: &str) -> Result<Self> {
        input.parse()
    }

    /// Load a manifest directly from bytes (UTF-8 JSON).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(bytes).map_err(|err| ManifestError::InvalidUtf8 {
            error: err.to_string(),
        })?;
        Self::from_json_str(text)
    }

    /// Load a manifest from a file on disk.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let data = fs::read(path)?;
        Self::from_bytes(&data)
    }

    /// Convenience helper returning true when the manifest declares storage support.
    pub fn has_storage(&self) -> bool {
        self.features.storage
    }

    /// Convenience helper returning true when the manifest declares payable support.
    pub fn is_payable(&self) -> bool {
        self.features.payable
    }
}

impl std::str::FromStr for ContractManifest {
    type Err = crate::error::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let manifest: ContractManifest = serde_json::from_str(s).map_err(ManifestError::from)?;
        Ok(manifest)
    }
}

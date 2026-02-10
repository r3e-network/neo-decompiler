use std::fs;
use std::io::Read;
use std::path::Path;

use crate::error::{ManifestError, Result};

use super::{ContractManifest, MAX_MANIFEST_SIZE};

fn validate_manifest_strict(manifest: &ContractManifest) -> Result<()> {
    for (index, permission) in manifest.permissions.iter().enumerate() {
        if let super::ManifestPermissionContract::Wildcard(value) = &permission.contract {
            if value != "*" {
                return Err(ManifestError::Validation {
                    message: format!(
                        "permissions[{index}].contract wildcard must be \"*\", got {value:?}"
                    ),
                }
                .into());
            }
        }

        if let super::ManifestPermissionMethods::Wildcard(value) = &permission.methods {
            if value != "*" {
                return Err(ManifestError::Validation {
                    message: format!(
                        "permissions[{index}].methods wildcard must be \"*\", got {value:?}"
                    ),
                }
                .into());
            }
        }
    }

    if let Some(super::ManifestTrusts::Wildcard(value)) = manifest.trusts.as_ref() {
        if value != "*" {
            return Err(ManifestError::Validation {
                message: format!("trusts wildcard must be \"*\", got {value:?}"),
            }
            .into());
        }
    }

    Ok(())
}

fn ensure_manifest_size(size: u64) -> Result<()> {
    if size > MAX_MANIFEST_SIZE {
        return Err(ManifestError::FileTooLarge {
            size,
            max: MAX_MANIFEST_SIZE,
        }
        .into());
    }
    Ok(())
}

impl ContractManifest {
    /// Load a manifest from a reader containing UTF-8 JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails, the payload exceeds the size limit,
    /// the bytes are not valid UTF-8, or the JSON does not match the manifest schema.
    pub fn from_reader<R: Read>(reader: R) -> Result<Self> {
        let mut buf = Vec::new();
        let mut limited = reader.take(MAX_MANIFEST_SIZE + 1);
        limited.read_to_end(&mut buf).map_err(ManifestError::from)?;
        Self::from_bytes(&buf)
    }

    /// Load a manifest from a raw JSON string.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON does not match the expected manifest schema.
    pub fn from_json_str(input: &str) -> Result<Self> {
        input.parse()
    }

    /// Load a manifest from a raw JSON string and enforce strict semantic validation.
    ///
    /// # Errors
    ///
    /// Returns an error if parsing fails or if wildcard-like fields contain
    /// non-canonical values (e.g., `"all"` instead of `"*"`).
    pub fn from_json_str_strict(input: &str) -> Result<Self> {
        let manifest = Self::from_json_str(input)?;
        validate_manifest_strict(&manifest)?;
        Ok(manifest)
    }

    /// Load a manifest directly from bytes (UTF-8 JSON).
    ///
    /// # Errors
    ///
    /// Returns an error if the payload exceeds the size limit, the bytes are
    /// not valid UTF-8, or the JSON does not match the manifest schema.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure_manifest_size(bytes.len() as u64)?;
        let text =
            std::str::from_utf8(bytes).map_err(|err| ManifestError::InvalidUtf8 { source: err })?;
        Self::from_json_str(text)
    }

    /// Load a manifest from a file on disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, exceeds the size limit,
    /// contains invalid UTF-8, or the JSON does not match the manifest schema.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let size = fs::metadata(&path)?.len();
        ensure_manifest_size(size)?;
        let data = fs::read(path)?;
        Self::from_bytes(&data)
    }

    /// Load a manifest from a file on disk and enforce strict semantic validation.
    ///
    /// # Errors
    ///
    /// Returns an error if parsing fails or if wildcard-like fields contain
    /// non-canonical values (e.g., `"all"` instead of `"*"`).
    pub fn from_file_strict<P: AsRef<Path>>(path: P) -> Result<Self> {
        let size = fs::metadata(&path)?.len();
        ensure_manifest_size(size)?;
        let data = fs::read(path)?;
        let manifest = Self::from_bytes(&data)?;
        validate_manifest_strict(&manifest)?;
        Ok(manifest)
    }

    /// Convenience helper returning true when the manifest declares storage support.
    #[must_use]
    pub fn has_storage(&self) -> bool {
        self.features.storage
    }

    /// Convenience helper returning true when the manifest declares payable support.
    #[must_use]
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

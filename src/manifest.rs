use std::fs;
use std::io::Read;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::error::{ManifestError, Result};

/// Representation of a Neo N3 contract manifest (`.manifest.json`).
#[derive(Debug, Clone, Deserialize)]
pub struct ContractManifest {
    pub name: String,
    #[serde(default)]
    pub groups: Vec<ManifestGroup>,
    #[serde(default)]
    pub features: ManifestFeatures,
    #[serde(default, rename = "supportedstandards")]
    pub supported_standards: Vec<String>,
    pub abi: ManifestAbi,
    #[serde(default)]
    pub permissions: Vec<ManifestPermission>,
    #[serde(default)]
    pub trusts: Option<ManifestTrusts>,
    #[serde(default)]
    pub extra: Option<Value>,
}

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

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestGroup {
    pub pubkey: String,
    pub signature: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ManifestFeatures {
    #[serde(default)]
    pub storage: bool,
    #[serde(default)]
    pub payable: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestAbi {
    #[serde(default)]
    pub methods: Vec<ManifestMethod>,
    #[serde(default)]
    pub events: Vec<ManifestEvent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestMethod {
    pub name: String,
    #[serde(default)]
    pub parameters: Vec<ManifestParameter>,
    #[serde(rename = "returntype")]
    pub return_type: String,
    #[serde(default)]
    pub offset: Option<u32>,
    #[serde(default)]
    pub safe: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestEvent {
    pub name: String,
    #[serde(default)]
    pub parameters: Vec<ManifestParameter>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestPermission {
    pub contract: ManifestPermissionContract,
    #[serde(default)]
    pub methods: ManifestPermissionMethods,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ManifestPermissionContract {
    Wildcard(String),
    Hash { hash: String },
    Group { group: String },
    Other(Value),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ManifestPermissionMethods {
    Wildcard(String),
    Methods(Vec<String>),
}

impl Default for ManifestPermissionMethods {
    fn default() -> Self {
        ManifestPermissionMethods::Wildcard("*".into())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ManifestTrusts {
    Wildcard(String),
    Contracts(Vec<String>),
    Other(Value),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest_json() -> &'static str {
        r#"
        {
            "name": "ExampleContract",
            "groups": [],
            "features": {
                "storage": true,
                "payable": false
            },
            "supportedstandards": [
                "NEP-17"
            ],
            "abi": {
                "methods": [
                    {
                        "name": "balanceOf",
                        "parameters": [
                            { "name": "account", "type": "Hash160" }
                        ],
                        "returntype": "Integer",
                        "offset": 0,
                        "safe": true
                    }
                ],
                "events": [
                    {
                        "name": "Transfer",
                        "parameters": [
                            { "name": "from", "type": "Hash160" },
                            { "name": "to", "type": "Hash160" },
                            { "name": "amount", "type": "Integer" }
                        ]
                    }
                ]
            },
            "permissions": [
                {
                    "contract": "*",
                    "methods": [
                        "balanceOf",
                        "transfer"
                    ]
                }
            ],
            "trusts": "*",
            "extra": null
        }
        "#
    }

    #[test]
    fn parses_manifest_json() {
        let manifest =
            ContractManifest::from_json_str(sample_manifest_json()).expect("manifest parsed");
        assert_eq!(manifest.name, "ExampleContract");
        assert!(manifest.has_storage());
        assert!(!manifest.is_payable());
        assert_eq!(manifest.supported_standards, vec!["NEP-17"]);
        assert_eq!(manifest.abi.methods.len(), 1);
        let method = &manifest.abi.methods[0];
        assert_eq!(method.name, "balanceOf");
        assert_eq!(method.return_type, "Integer");
        assert_eq!(method.parameters.len(), 1);
    }
}

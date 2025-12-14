use serde::Deserialize;
use serde_json::Value;

use super::abi::ManifestAbi;
use super::permissions::ManifestPermission;
use super::trusts::ManifestTrusts;

/// Representation of a Neo N3 contract manifest (`.manifest.json`).
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct ContractManifest {
    /// Contract name.
    pub name: String,
    /// Optional signature groups.
    #[serde(default)]
    pub groups: Vec<ManifestGroup>,
    /// Declared feature flags.
    #[serde(default)]
    pub features: ManifestFeatures,
    /// Supported standards declared by the contract (e.g. `NEP-17`).
    #[serde(default, rename = "supportedstandards")]
    pub supported_standards: Vec<String>,
    /// ABI metadata (methods/events).
    pub abi: ManifestAbi,
    /// Permission entries restricting contract and method calls.
    #[serde(default)]
    pub permissions: Vec<ManifestPermission>,
    /// Optional trust configuration.
    #[serde(default)]
    pub trusts: Option<ManifestTrusts>,
    /// Arbitrary extra data carried by the manifest.
    #[serde(default)]
    pub extra: Option<Value>,
}

/// Public-key group entry authorizing deployment/signature checks.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestGroup {
    /// Public key associated with the group.
    pub pubkey: String,
    /// Signature for the group.
    pub signature: String,
}

/// Manifest feature flags indicating supported runtime capabilities.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ManifestFeatures {
    /// Whether the contract uses storage.
    #[serde(default)]
    pub storage: bool,
    /// Whether the contract is payable.
    #[serde(default)]
    pub payable: bool,
}

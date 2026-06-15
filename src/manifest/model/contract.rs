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
    /// Raw `features` object carried by the manifest.
    ///
    /// Neo N3's `ContractManifest.FromJson` requires this to be an empty
    /// JSON object (the legacy 2.x `storage`/`payable` flags do not exist in
    /// N3). The raw map is kept so tolerant parsing can surface whatever a
    /// malformed manifest declared; strict parsing rejects non-empty objects.
    #[serde(default)]
    pub features: serde_json::Map<String, Value>,
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

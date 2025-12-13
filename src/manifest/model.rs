use serde::Deserialize;
use serde_json::Value;

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

/// ABI section describing contract methods and events.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct ManifestAbi {
    /// Exposed contract methods.
    #[serde(default)]
    pub methods: Vec<ManifestMethod>,
    /// Exposed contract events.
    #[serde(default)]
    pub events: Vec<ManifestEvent>,
}

/// ABI method metadata for a contract entry point.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ManifestMethod {
    /// Method name.
    pub name: String,
    /// Method parameters.
    #[serde(default)]
    pub parameters: Vec<ManifestParameter>,
    /// Return type identifier.
    #[serde(rename = "returntype")]
    pub return_type: String,
    /// Optional bytecode offset for the method entry point.
    #[serde(default)]
    pub offset: Option<u32>,
    /// Whether the method is declared as safe.
    #[serde(default)]
    pub safe: bool,
}

/// ABI parameter metadata for a manifest method/event.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestParameter {
    /// Parameter name.
    pub name: String,
    /// Parameter type identifier.
    #[serde(rename = "type")]
    pub kind: String,
}

/// ABI event metadata describing emitted notifications.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestEvent {
    /// Event name.
    pub name: String,
    /// Event parameters.
    #[serde(default)]
    pub parameters: Vec<ManifestParameter>,
}

/// Manifest permission entry restricting contract/method calls.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestPermission {
    /// Which contract(s) this permission applies to.
    pub contract: ManifestPermissionContract,
    /// Which method(s) this permission applies to.
    #[serde(default)]
    pub methods: ManifestPermissionMethods,
}

/// Contract selector for a permission entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ManifestPermissionContract {
    /// Wildcard selector (typically `"*"`).
    Wildcard(String),
    /// Hash selector (usually a script hash).
    Hash {
        /// Hash string as provided by the manifest.
        hash: String,
    },
    /// Group selector (usually a public key).
    Group {
        /// Group identifier as provided by the manifest.
        group: String,
    },
    /// Any other value not covered by the known variants.
    Other(Value),
}

/// Method selector for a permission entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ManifestPermissionMethods {
    /// Wildcard selector (typically `"*"`).
    Wildcard(String),
    /// Explicit list of allowed methods.
    Methods(Vec<String>),
}

impl Default for ManifestPermissionMethods {
    fn default() -> Self {
        ManifestPermissionMethods::Wildcard("*".into())
    }
}

/// Manifest trust configuration controlling which contracts are trusted.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ManifestTrusts {
    /// Wildcard trusts configuration (typically `"*"`).
    Wildcard(String),
    /// Explicit list of trusted contracts.
    Contracts(Vec<String>),
    /// Any other value not covered by the known variants.
    Other(Value),
}

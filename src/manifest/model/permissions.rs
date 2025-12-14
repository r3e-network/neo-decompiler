use serde::Deserialize;
use serde_json::Value;

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

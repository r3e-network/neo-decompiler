use serde::Deserialize;
use serde_json::Value;

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

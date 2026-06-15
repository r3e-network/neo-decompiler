use serde::{Deserialize, Deserializer};
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
///
/// The official `ContractPermissionDescriptor.ToJson()` encodes the
/// `contract` field as a plain string and `FromJson()` classifies it by
/// shape:
///
/// - `"*"` — wildcard,
/// - 42 characters, `0x` prefix + 40 hex digits — contract hash,
/// - 66 hex characters — group public key,
/// - anything else — `FormatException`.
///
/// Tolerant parsing keeps malformed descriptors as [`Self::Other`] (the
/// same catch-all used for other malformed manifest fields); strict
/// parsing rejects them.
#[derive(Debug, Clone)]
pub enum ManifestPermissionContract {
    /// Wildcard selector (`"*"`).
    Wildcard(String),
    /// Hash selector (a `0x`-prefixed contract script hash).
    Hash {
        /// Hash string as provided by the manifest.
        hash: String,
    },
    /// Group selector (a 33-byte public key as 66 hex characters).
    Group {
        /// Group identifier as provided by the manifest.
        group: String,
    },
    /// Any other value: a malformed descriptor the official parser rejects.
    Other(Value),
}

impl ManifestPermissionContract {
    fn classify(value: Value) -> Self {
        if let Value::String(text) = &value {
            if text == "*" {
                return ManifestPermissionContract::Wildcard(text.clone());
            }
            if is_hash_descriptor(text) {
                return ManifestPermissionContract::Hash { hash: text.clone() };
            }
            if is_group_descriptor(text) {
                return ManifestPermissionContract::Group {
                    group: text.clone(),
                };
            }
        }
        ManifestPermissionContract::Other(value)
    }
}

fn is_hash_descriptor(text: &str) -> bool {
    text.len() == 42
        && (text.starts_with("0x") || text.starts_with("0X"))
        && text.as_bytes()[2..].iter().all(u8::is_ascii_hexdigit)
}

fn is_group_descriptor(text: &str) -> bool {
    text.len() == 66 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl<'de> Deserialize<'de> for ManifestPermissionContract {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Ok(ManifestPermissionContract::classify(value))
    }
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

use crate::manifest::{ContractManifest, ManifestPermission};

/// Extract and sanitize the contract name from a manifest, falling back to
/// `"NeoContract"` when the manifest is absent or the name is empty.
///
/// The caller supplies a `sanitizer` function so that different renderers
/// (high-level vs C#) can apply their own identifier rules.
pub(in super::super) fn extract_contract_name(
    manifest: Option<&ContractManifest>,
    sanitizer: fn(&str) -> String,
) -> String {
    manifest
        .and_then(|m| {
            let trimmed = m.name.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .map(sanitizer)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "NeoContract".to_string())
}

/// Case-insensitive lookup for manifest extra strings such as Author/Email.
pub(in super::super) fn manifest_extra_string(
    manifest: &ContractManifest,
    key: &str,
) -> Option<String> {
    let extra = manifest.extra.as_ref()?;
    let map = match extra {
        serde_json::Value::Object(map) => map,
        _ => return None,
    };
    let target = key.to_ascii_lowercase();
    map.iter()
        .find(|(candidate, _)| candidate.to_ascii_lowercase() == target)
        .and_then(|(_, value)| value.as_str().map(ToString::to_string))
}

/// Render a permission entry as used in the high-level and C# comments.
pub(in super::super) fn format_permission_entry(permission: &ManifestPermission) -> String {
    format!(
        "contract={} methods={}",
        permission.contract.describe(),
        permission.methods.describe()
    )
}

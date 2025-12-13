use crate::manifest::{ContractManifest, ManifestPermission};

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
        .and_then(|(_, value)| value.as_str().map(|s| s.to_string()))
}

/// Render a permission entry as used in the high-level and C# comments.
pub(in super::super) fn format_permission_entry(permission: &ManifestPermission) -> String {
    format!(
        "contract={} methods={}",
        permission.contract.describe(),
        permission.methods.describe()
    )
}

use serde_json::Value;

use super::{ManifestPermissionContract, ManifestPermissionMethods, ManifestTrusts};

impl ManifestPermissionContract {
    /// Return a human-friendly representation of the permission contract selector.
    ///
    /// This helper is primarily used for diagnostics and CLI output. The returned
    /// string is stable and intentionally compact:
    ///
    /// - Wildcard values are returned verbatim (typically `"*"`).
    /// - Hash selectors are returned as `hash:<value>`.
    /// - Group selectors are returned as `group:<value>`.
    ///
    /// # Examples
    /// ```
    /// use neo_decompiler::manifest::ManifestPermissionContract;
    ///
    /// let contract = ManifestPermissionContract::Wildcard("*".into());
    /// assert_eq!(contract.describe(), "*");
    /// ```
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            ManifestPermissionContract::Wildcard(value) => value.clone(),
            ManifestPermissionContract::Hash { hash } => format!("hash:{hash}"),
            ManifestPermissionContract::Group { group } => format!("group:{group}"),
            ManifestPermissionContract::Other(value) => value.to_string(),
        }
    }
}

impl ManifestPermissionMethods {
    /// Return a human-friendly representation of the permission method selector.
    ///
    /// This helper is primarily used for diagnostics and CLI output:
    ///
    /// - Wildcard values are returned verbatim (typically `"*"`).
    /// - Explicit method lists are rendered as a JSON-like list of quoted strings.
    ///
    /// # Examples
    /// ```
    /// use neo_decompiler::manifest::ManifestPermissionMethods;
    ///
    /// let methods = ManifestPermissionMethods::Wildcard("*".into());
    /// assert_eq!(methods.describe(), "*");
    /// ```
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            ManifestPermissionMethods::Wildcard(value) => value.clone(),
            ManifestPermissionMethods::Methods(methods) => {
                if methods.is_empty() {
                    "[]".into()
                } else {
                    let labelled: Vec<String> = methods
                        .iter()
                        .map(|method| format!("\"{method}\""))
                        .collect();
                    format!("[{}]", labelled.join(", "))
                }
            }
        }
    }
}

impl ManifestTrusts {
    /// Return a human-friendly representation of the manifest `trusts` field.
    ///
    /// This helper is primarily used for diagnostics and CLI output:
    ///
    /// - Wildcard values are returned verbatim (typically `"*"`).
    /// - Explicit contract lists are rendered as a JSON-like list of quoted strings.
    /// - Object forms (`{"hashes": [...], "groups": [...]}`) — the
    ///   canonical N3 manifest "structured trusts" — are flattened
    ///   to a list of typed entries (`[hash:0x..., group:02...]`)
    ///   so they read consistently with the
    ///   `ManifestPermissionContract::describe()` output.
    ///
    /// # Examples
    /// ```
    /// use neo_decompiler::manifest::ManifestTrusts;
    ///
    /// let trusts = ManifestTrusts::Wildcard("*".into());
    /// assert_eq!(trusts.describe(), "*");
    /// ```
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            ManifestTrusts::Wildcard(value) => value.clone(),
            ManifestTrusts::Contracts(values) => {
                if values.is_empty() {
                    "[]".into()
                } else {
                    let labelled: Vec<String> =
                        values.iter().map(|value| format!("\"{value}\"")).collect();
                    format!("[{}]", labelled.join(", "))
                }
            }
            ManifestTrusts::Other(value) => describe_other_trusts_value(value),
        }
    }
}

fn describe_other_trusts_value(value: &Value) -> String {
    if let Some(rendered) = describe_structured_trusts(value) {
        return rendered;
    }
    value.to_string()
}

fn describe_structured_trusts(value: &Value) -> Option<String> {
    let object = value.as_object()?;

    let unknown_keys = object.keys().any(|key| key != "hashes" && key != "groups");
    if unknown_keys {
        return None;
    }

    let hashes = parse_typed_entries(object.get("hashes"), "hash")?;
    let groups = parse_typed_entries(object.get("groups"), "group")?;

    let mut entries: Vec<String> = Vec::new();
    entries.extend(hashes);
    entries.extend(groups);

    Some(format!("[{}]", entries.join(", ")))
}

fn parse_typed_entries(value: Option<&Value>, prefix: &str) -> Option<Vec<String>> {
    match value {
        None => Some(Vec::new()),
        Some(value) => {
            let array = value.as_array()?;
            collect_string_entries(array, prefix).ok()
        }
    }
}

fn collect_string_entries(values: &[Value], prefix: &str) -> Result<Vec<String>, ()> {
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        match value.as_str() {
            Some(string) => out.push(format!("{prefix}:{string}")),
            None => return Err(()),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_trusts_object_with_groups_only_renders_typed_list() {
        let value: Value = serde_json::from_str(r#"{"groups":["02abcdef"]}"#).unwrap();
        let trusts = ManifestTrusts::Other(value);
        assert_eq!(trusts.describe(), "[group:02abcdef]");
    }

    #[test]
    fn structured_trusts_object_with_hashes_only_renders_typed_list() {
        let value: Value =
            serde_json::from_str(r#"{"hashes":["0x0123456789abcdef0123456789abcdef01234567"]}"#)
                .unwrap();
        let trusts = ManifestTrusts::Other(value);
        assert_eq!(
            trusts.describe(),
            "[hash:0x0123456789abcdef0123456789abcdef01234567]",
        );
    }

    #[test]
    fn structured_trusts_object_with_both_keys_concatenates_in_order() {
        let value: Value =
            serde_json::from_str(r#"{"hashes":["0xabc"],"groups":["02def"]}"#).unwrap();
        let trusts = ManifestTrusts::Other(value);
        assert_eq!(trusts.describe(), "[hash:0xabc, group:02def]");
    }

    #[test]
    fn structured_trusts_with_unknown_key_falls_back_to_raw_json() {
        let value: Value =
            serde_json::from_str(r#"{"groups":["02abcdef"],"unexpected":1}"#).unwrap();
        let trusts = ManifestTrusts::Other(value);
        // Unknown keys mean we don't fully understand the shape;
        // fall back to raw JSON so we don't silently drop data.
        let rendered = trusts.describe();
        assert!(rendered.contains("unexpected"), "rendered: {rendered}");
        assert!(rendered.contains("02abcdef"), "rendered: {rendered}");
    }

    #[test]
    fn structured_trusts_non_string_array_falls_back_to_raw_json() {
        // A `hashes` array containing a number is malformed per the
        // N3 spec — refuse to flatten and surface the raw JSON so
        // the user sees the anomaly.
        let value: Value = serde_json::from_str(r#"{"hashes":[42]}"#).unwrap();
        let trusts = ManifestTrusts::Other(value);
        let rendered = trusts.describe();
        assert!(rendered.contains("42"), "rendered: {rendered}");
    }

    #[test]
    fn wildcard_trusts_describes_as_star() {
        let trusts = ManifestTrusts::Wildcard("*".into());
        assert_eq!(trusts.describe(), "*");
    }

    #[test]
    fn empty_contracts_trusts_describes_as_empty_brackets() {
        let trusts = ManifestTrusts::Contracts(Vec::new());
        assert_eq!(trusts.describe(), "[]");
    }
}

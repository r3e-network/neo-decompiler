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
            ManifestTrusts::Other(value) => value.to_string(),
        }
    }
}

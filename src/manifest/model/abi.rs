use serde::Deserialize;

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
    ///
    /// Neo N3 uses `-1` to indicate "no implementation" (abstract/interface).
    /// We deserialize as `i32` so that `-1` round-trips correctly, and treat
    /// negative values as "no offset" in downstream consumers.
    #[serde(default)]
    pub offset: Option<i32>,
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

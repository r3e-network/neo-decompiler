use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub(in crate::cli) struct ManifestSummary {
    pub(super) name: String,
    pub(super) supported_standards: Vec<String>,
    pub(super) storage: bool,
    pub(super) payable: bool,
    pub(super) groups: Vec<GroupSummary>,
    pub(super) methods: usize,
    pub(super) events: usize,
    pub(super) permissions: Vec<PermissionSummary>,
    pub(super) trusts: Option<TrustSummary>,
    pub(super) abi: AbiSummary,
}

#[derive(Serialize)]
pub(in crate::cli) struct GroupSummary {
    pub(super) pubkey: String,
    pub(super) signature: String,
}

#[derive(Serialize)]
pub(in crate::cli) struct PermissionSummary {
    pub(super) contract: PermissionContractSummary,
    pub(super) methods: PermissionMethodsSummary,
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value")]
pub(in crate::cli) enum PermissionContractSummary {
    Wildcard(String),
    Hash(String),
    Group(String),
    Other(Value),
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value")]
pub(in crate::cli) enum PermissionMethodsSummary {
    Wildcard(String),
    Methods(Vec<String>),
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value")]
pub(in crate::cli) enum TrustSummary {
    Wildcard(String),
    Contracts(Vec<String>),
    Other(Value),
}

#[derive(Serialize)]
pub(in crate::cli) struct AbiSummary {
    pub(super) methods: Vec<MethodSummary>,
    pub(super) events: Vec<EventSummary>,
}

#[derive(Serialize)]
pub(in crate::cli) struct MethodSummary {
    pub(super) name: String,
    pub(super) parameters: Vec<ParameterSummary>,
    pub(super) return_type: String,
    pub(super) safe: bool,
    pub(super) offset: Option<u32>,
}

#[derive(Serialize)]
pub(in crate::cli) struct EventSummary {
    pub(super) name: String,
    pub(super) parameters: Vec<ParameterSummary>,
}

#[derive(Serialize)]
pub(in crate::cli) struct ParameterSummary {
    pub(super) name: String,
    pub(super) ty: String,
}

use crate::manifest::{ManifestPermissionContract, ManifestPermissionMethods, ManifestTrusts};

use super::{PermissionContractSummary, PermissionMethodsSummary, TrustSummary};

impl From<&ManifestPermissionContract> for PermissionContractSummary {
    fn from(contract: &ManifestPermissionContract) -> Self {
        match contract {
            ManifestPermissionContract::Wildcard(value) => {
                PermissionContractSummary::Wildcard(value.clone())
            }
            ManifestPermissionContract::Hash { hash } => {
                PermissionContractSummary::Hash(hash.clone())
            }
            ManifestPermissionContract::Group { group } => {
                PermissionContractSummary::Group(group.clone())
            }
            ManifestPermissionContract::Other(value) => {
                PermissionContractSummary::Other(value.clone())
            }
        }
    }
}

impl From<&ManifestPermissionMethods> for PermissionMethodsSummary {
    fn from(methods: &ManifestPermissionMethods) -> Self {
        match methods {
            ManifestPermissionMethods::Wildcard(value) => {
                PermissionMethodsSummary::Wildcard(value.clone())
            }
            ManifestPermissionMethods::Methods(list) => {
                PermissionMethodsSummary::Methods(list.clone())
            }
        }
    }
}

impl From<&ManifestTrusts> for TrustSummary {
    fn from(trusts: &ManifestTrusts) -> Self {
        match trusts {
            ManifestTrusts::Wildcard(value) => TrustSummary::Wildcard(value.clone()),
            ManifestTrusts::Contracts(values) => TrustSummary::Contracts(values.clone()),
            ManifestTrusts::Other(value) => TrustSummary::Other(value.clone()),
        }
    }
}

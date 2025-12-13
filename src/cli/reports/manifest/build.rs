use crate::manifest::ContractManifest;

use super::{
    AbiSummary, EventSummary, GroupSummary, ManifestSummary, MethodSummary, ParameterSummary,
    PermissionContractSummary, PermissionMethodsSummary, PermissionSummary, TrustSummary,
};

pub(in crate::cli) fn summarize_manifest(manifest: &ContractManifest) -> ManifestSummary {
    ManifestSummary {
        name: manifest.name.clone(),
        supported_standards: manifest.supported_standards.clone(),
        storage: manifest.features.storage,
        payable: manifest.features.payable,
        groups: manifest
            .groups
            .iter()
            .map(|group| GroupSummary {
                pubkey: group.pubkey.clone(),
                signature: group.signature.clone(),
            })
            .collect(),
        methods: manifest.abi.methods.len(),
        events: manifest.abi.events.len(),
        permissions: manifest
            .permissions
            .iter()
            .map(|permission| PermissionSummary {
                contract: PermissionContractSummary::from(&permission.contract),
                methods: PermissionMethodsSummary::from(&permission.methods),
            })
            .collect(),
        trusts: manifest.trusts.as_ref().map(TrustSummary::from),
        abi: AbiSummary {
            methods: manifest
                .abi
                .methods
                .iter()
                .map(|method| MethodSummary {
                    name: method.name.clone(),
                    parameters: method
                        .parameters
                        .iter()
                        .map(|param| ParameterSummary {
                            name: param.name.clone(),
                            ty: param.kind.clone(),
                        })
                        .collect(),
                    return_type: method.return_type.clone(),
                    safe: method.safe,
                    offset: method.offset,
                })
                .collect(),
            events: manifest
                .abi
                .events
                .iter()
                .map(|event| EventSummary {
                    name: event.name.clone(),
                    parameters: event
                        .parameters
                        .iter()
                        .map(|param| ParameterSummary {
                            name: param.name.clone(),
                            ty: param.kind.clone(),
                        })
                        .collect(),
                })
                .collect(),
        },
    }
}

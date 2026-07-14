use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;

use crate::manifest::ContractManifest;

use super::super::helpers::{
    escape_csharp_string, format_manifest_type_csharp, make_unique_identifier,
    sanitize_csharp_identifier,
};

pub(crate) type EventSignatures = BTreeMap<String, (String, Vec<String>)>;

pub(crate) fn event_signatures(
    manifest: &ContractManifest,
    reserved_names: &HashSet<String>,
) -> EventSignatures {
    let mut used_names = reserved_names.clone();
    manifest
        .abi
        .events
        .iter()
        .map(|event| {
            let emitted_name =
                make_unique_identifier(sanitize_csharp_identifier(&event.name), &mut used_names);
            let parameter_types = event
                .parameters
                .iter()
                .map(|parameter| format_manifest_type_csharp(&parameter.kind, false))
                .collect();
            (event.name.clone(), (emitted_name, parameter_types))
        })
        .collect()
}

pub(super) fn write_events(
    output: &mut String,
    manifest: &ContractManifest,
    signatures: &EventSignatures,
) {
    if manifest.abi.events.is_empty() {
        return;
    }

    writeln!(output, "        // Events").unwrap();
    for event in &manifest.abi.events {
        let (event_name, param_types) = signatures
            .get(&event.name)
            .expect("event signature is built from the same manifest event");
        if event_name != &event.name {
            writeln!(
                output,
                "        [DisplayName(\"{}\")]",
                escape_csharp_string(&event.name)
            )
            .unwrap();
        }
        let action_ty = if param_types.is_empty() {
            "Action".to_string()
        } else {
            format!("Action<{}>", param_types.join(", "))
        };
        writeln!(
            output,
            "        public static event {action_ty} {event_name};"
        )
        .unwrap();
    }
    writeln!(output).unwrap();
}

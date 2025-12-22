use std::collections::HashSet;
use std::fmt::Write;

use crate::manifest::ContractManifest;

use super::super::helpers::{
    escape_csharp_string, format_manifest_type_csharp, make_unique_identifier,
    sanitize_csharp_identifier,
};

pub(super) fn write_events(output: &mut String, manifest: &ContractManifest) {
    if manifest.abi.events.is_empty() {
        return;
    }

    writeln!(output, "        // Events").unwrap();
    let mut used_names = HashSet::new();
    for event in &manifest.abi.events {
        let event_name =
            make_unique_identifier(sanitize_csharp_identifier(&event.name), &mut used_names);
        if event_name != event.name {
            writeln!(
                output,
                "        [DisplayName(\"{}\")]",
                escape_csharp_string(&event.name)
            )
            .unwrap();
        }
        let param_types: Vec<String> = event
            .parameters
            .iter()
            .map(|p| format_manifest_type_csharp(&p.kind))
            .collect();
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

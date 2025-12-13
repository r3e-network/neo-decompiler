use crate::manifest::ManifestParameter;

use super::{format_manifest_type, sanitize_identifier};

/// Format ABI parameters into `name: type` pseudo-signature entries.
///
/// Parameter names are sanitized to yield stable identifiers in the high-level view
/// and match argument labels used inside lifted method bodies.
pub(in super::super) fn format_manifest_parameters(parameters: &[ManifestParameter]) -> String {
    parameters
        .iter()
        .map(|param| {
            let name = sanitize_identifier(&param.name);
            format!("{name}: {}", format_manifest_type(&param.kind))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

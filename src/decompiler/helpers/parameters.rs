use std::collections::HashSet;

use crate::manifest::ManifestParameter;

use super::{format_manifest_type, make_unique_identifier, sanitize_identifier};

/// Format ABI parameters into `name: type` pseudo-signature entries.
///
/// Parameter names are sanitized to yield stable identifiers in the high-level view
/// and match argument labels used inside lifted method bodies.
pub(in super::super) fn format_manifest_parameters(parameters: &[ManifestParameter]) -> String {
    let names = sanitize_parameter_names(parameters);
    parameters
        .iter()
        .zip(names)
        .map(|(param, name)| format!("{name}: {}", format_manifest_type(&param.kind)))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(in super::super) fn sanitize_parameter_names(
    parameters: &[ManifestParameter],
) -> Vec<String> {
    let mut used = HashSet::new();
    parameters
        .iter()
        .map(|param| make_unique_identifier(sanitize_identifier(&param.name), &mut used))
        .collect()
}

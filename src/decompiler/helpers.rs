//! Small helper utilities shared across decompiler renderers.

mod identifiers;
mod manifest;
mod methods;
mod parameters;
mod types;

pub(super) use identifiers::{make_unique_identifier, sanitize_identifier};
pub(super) use manifest::{format_permission_entry, manifest_extra_string};
pub(super) use methods::{find_manifest_entry_method, next_method_offset};
pub(super) use parameters::{format_manifest_parameters, sanitize_parameter_names};
pub(super) use types::format_manifest_type;

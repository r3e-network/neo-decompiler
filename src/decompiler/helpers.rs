//! Small helper utilities shared across decompiler renderers.

mod identifiers;
mod manifest;
mod methods;
mod parameters;
mod types;

pub(super) use identifiers::{make_unique_identifier, sanitize_identifier};
pub(super) use manifest::{extract_contract_name, format_permission_entry, manifest_extra_string};
pub(super) use methods::{
    find_manifest_entry_method, has_manifest_method_at_offset, next_method_offset, offset_as_usize,
};
pub(super) use parameters::{format_manifest_parameters, sanitize_parameter_names};
pub(super) use types::format_manifest_type;

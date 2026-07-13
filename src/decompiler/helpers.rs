//! Small helper utilities shared across decompiler renderers.

mod identifiers;
mod lifted;
mod manifest;
mod methods;
mod parameters;
mod types;

pub(super) use identifiers::{make_unique_identifier, sanitize_identifier};
pub(super) use lifted::{
    build_call_targets_by_offset, build_calla_targets_by_offset, build_method_arg_counts_by_offset,
    build_method_returns_value_by_offset,
};
pub(super) use manifest::{extract_contract_name, format_permission_entry, render_extra_scalar};
pub(super) use methods::{
    build_method_labels_by_offset, collect_call_targets, collect_initslot_offsets,
    find_manifest_entry_method, inferred_method_starts, initslot_argument_count_at,
    next_inferred_method_offset, offset_as_usize,
};
pub(super) use parameters::{format_manifest_parameters, sanitize_parameter_names};
pub(super) use types::{format_manifest_type, inferred_type_to_csharp};

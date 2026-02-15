use crate::manifest::{ContractManifest, ManifestMethod};

/// Return the ABI method that matches the script entry offset, falling back to
/// the first ABI method when offsets are missing.
pub(in super::super) fn find_manifest_entry_method(
    manifest: &ContractManifest,
    entry_offset: usize,
) -> Option<(&ManifestMethod, bool)> {
    manifest
        .abi
        .methods
        .iter()
        .find(|method| offset_as_usize(method.offset) == Some(entry_offset))
        .map(|method| (method, true))
}

/// Return `true` when at least one manifest method starts at `offset`.
pub(in super::super) fn has_manifest_method_at_offset(
    manifest: &ContractManifest,
    offset: usize,
) -> bool {
    manifest
        .abi
        .methods
        .iter()
        .any(|method| offset_as_usize(method.offset) == Some(offset))
}

/// Compute the next ABI method offset after the given one.
pub(in super::super) fn next_method_offset(
    manifest: &ContractManifest,
    current_offset: Option<i32>,
) -> Option<usize> {
    let current = offset_as_usize(current_offset)?;
    manifest
        .abi
        .methods
        .iter()
        .filter_map(|method| offset_as_usize(method.offset))
        .filter(|offset| *offset > current)
        .min()
}

/// Convert a manifest offset (`Option<i32>`) to `Option<usize>`, treating
/// negative values (e.g. `-1` for abstract methods) as `None`.
pub(in super::super) fn offset_as_usize(offset: Option<i32>) -> Option<usize> {
    offset.and_then(|v| usize::try_from(v).ok())
}

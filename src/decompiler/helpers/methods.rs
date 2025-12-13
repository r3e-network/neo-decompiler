use crate::manifest::{ContractManifest, ManifestMethod};

/// Return the ABI method that matches the script entry offset, falling back to
/// the first ABI method when offsets are missing.
pub(in super::super) fn find_manifest_entry_method(
    manifest: &ContractManifest,
    entry_offset: usize,
) -> Option<(&ManifestMethod, bool)> {
    if let Some(method) = manifest
        .abi
        .methods
        .iter()
        .find(|method| method.offset.map(|value| value as usize) == Some(entry_offset))
    {
        return Some((method, true));
    }

    let fallback = manifest
        .abi
        .methods
        .iter()
        .filter_map(|method| method.offset.map(|offset| (offset as usize, method)))
        .min_by_key(|(offset, _)| *offset)
        .map(|(_, method)| method)
        .or_else(|| manifest.abi.methods.first())?;

    Some((fallback, false))
}

/// Compute the next ABI method offset after the given one.
pub(in super::super) fn next_method_offset(
    manifest: &ContractManifest,
    current_offset: Option<u32>,
) -> Option<usize> {
    let current = current_offset?;
    manifest
        .abi
        .methods
        .iter()
        .filter_map(|method| method.offset)
        .filter(|offset| *offset > current)
        .min()
        .map(|offset| offset as usize)
}

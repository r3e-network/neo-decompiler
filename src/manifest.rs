//! Neo N3 contract manifest parsing and helpers.

/// Neo N3 specifies `MaxManifestSize = 0xFFFF` (65535 bytes).
const MAX_MANIFEST_SIZE: u64 = 0xFFFF;

mod describe;
mod model;
mod parse;

pub use model::{
    ContractManifest, ManifestAbi, ManifestEvent, ManifestFeatures, ManifestGroup, ManifestMethod,
    ManifestParameter, ManifestPermission, ManifestPermissionContract, ManifestPermissionMethods,
    ManifestTrusts,
};

#[cfg(test)]
mod tests;

//! Neo N3 contract manifest parsing and helpers.

const MAX_MANIFEST_SIZE: u64 = 1024 * 1024;

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

//! Neo N3 contract manifest parsing and helpers.

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

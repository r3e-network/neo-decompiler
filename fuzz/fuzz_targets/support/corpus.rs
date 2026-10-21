//! Runtime loading of the real-contract corpus for the fuzz targets.
//!
//! The corpus is not embedded with `include_bytes!` for two reasons:
//!
//! * The pinned devpack set is generated, not version controlled. Embedding
//!   it would make the fuzz targets fail to compile on a clean checkout.
//! * Loading at runtime lets the same binary fuzz against whatever corpus is
//!   present, so a local run and a CI run can use different sets.
//!
//! The corpus location comes from `NEO_CSHARP_CORPUS_DIR` when it is set,
//! and otherwise from the repository's `TestingArtifacts` directory, which
//! holds both the devpack set and the committed edge-case fixtures.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A contract loaded from the corpus.
pub struct Contract {
    /// The container bytes.
    pub nef: Vec<u8>,
    /// The manifest that accompanies the contract, when there is one.
    pub manifest: Option<String>,
}

impl Contract {
    /// The manifest as a parsed value, when the manifest is present and
    /// well formed.
    ///
    /// A manifest that fails to parse is not an error here: the fuzz targets
    /// care about the contract, and a manifest that the parser rejects is
    /// simply not used.
    #[must_use]
    pub fn parsed_manifest(&self) -> Option<neo_decompiler::ContractManifest> {
        let manifest = self.manifest.as_ref()?;
        neo_decompiler::ContractManifest::from_json_str(manifest).ok()
    }
}

/// The directories the corpus is searched in, in priority order.
fn corpus_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // The environment variable wins, so CI and a local run can point the
    // targets at a different corpus without a rebuild.
    if let Ok(dir) = std::env::var("NEO_CSHARP_CORPUS_DIR") {
        if !dir.is_empty() {
            dirs.push(PathBuf::from(dir));
        }
    }

    // The repository's artifact directory. The manifest directory is the
    // fuzz crate's directory, so the repository is its parent. This is
    // resolved at compile time: the environment variable is not guaranteed
    // to be set when the built binary runs.
    let fuzz_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    if let Some(root) = fuzz_dir.parent() {
        dirs.push(root.join("TestingArtifacts/devpack"));
        dirs.push(root.join("TestingArtifacts/edgecases"));
    }

    dirs
}

/// Collect the `.nef` files under a directory tree.
fn collect_nef(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_nef(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("nef") {
            out.push(path);
        }
    }
}

/// The contracts available for mutation.
///
/// The list is loaded once and cached, because a fuzz campaign calls this on
/// every input and re-reading the corpus each time would dominate the run.
pub fn contracts() -> &'static [Contract] {
    static CONTRACTS: OnceLock<Vec<Contract>> = OnceLock::new();
    CONTRACTS.get_or_init(load_contracts)
}

/// Load every contract the corpus directories hold.
fn load_contracts() -> Vec<Contract> {
    let mut paths = Vec::new();
    for dir in corpus_dirs() {
        collect_nef(&dir, &mut paths);
    }
    paths.sort();
    paths.dedup();

    let mut contracts = Vec::with_capacity(paths.len());
    for path in paths {
        let Ok(nef) = std::fs::read(&path) else {
            continue;
        };
        // The manifest sits next to the container with the same stem.
        let manifest_path = path.with_extension("manifest.json");
        let manifest = std::fs::read_to_string(&manifest_path).ok();
        contracts.push(Contract { nef, manifest });
    }
    contracts
}

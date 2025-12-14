use std::fs;
use std::path::{Component, Path, PathBuf};

pub(crate) fn create_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create output directories");
    }
}

pub(crate) fn assert_non_empty(path: &Path, msg: &str) {
    assert!(path.is_file(), "{msg}: {}", path.display());
    let contents = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    assert!(!contents.trim().is_empty(), "{msg}: {}", path.display());
}

pub(crate) fn relative_base(path: &Path, root: &Path) -> PathBuf {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.with_extension("")
}

pub(crate) fn format_id(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(os) => os.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

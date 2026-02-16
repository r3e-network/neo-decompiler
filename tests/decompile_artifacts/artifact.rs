use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::common::{format_id, relative_base};

#[derive(PartialEq, Eq)]
pub(crate) enum ContractStatus {
    Success,
    KnownUnsupported,
}

#[derive(Debug)]
pub(crate) enum Artifact {
    CSharp {
        path: PathBuf,
    },
    NefManifest {
        nef_path: PathBuf,
        manifest_path: PathBuf,
    },
}

#[derive(Debug)]
pub(crate) struct ArtifactEntry {
    pub(crate) kind: Artifact,
    pub(crate) id: String,
    pub(crate) output_base: PathBuf,
}

pub(crate) fn collect_artifacts(artifacts_dir: &Path, output_dir: &Path) -> Vec<ArtifactEntry> {
    let mut artifacts = Vec::new();
    let mut stack = vec![artifacts_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("list artifacts") {
            let entry = entry.expect("artifact entry");
            let path = entry.path();
            if path.starts_with(output_dir) {
                continue;
            }
            if entry.file_type().expect("file type").is_dir() {
                // Skip "sources" and "decompiled" directories — they contain
                // plain C# reference/output files, not artifacts with embedded NEF.
                if let Some("sources" | "decompiled") = path.file_name().and_then(OsStr::to_str) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            match path.extension().and_then(OsStr::to_str) {
                Some("cs") => {
                    let rel_base = relative_base(&path, artifacts_dir);
                    artifacts.push(ArtifactEntry {
                        id: format_id(&rel_base),
                        output_base: output_dir.join(&rel_base),
                        kind: Artifact::CSharp { path },
                    });
                }
                Some("nef") => {
                    let rel_base = relative_base(&path, artifacts_dir);
                    let manifest_path = path.with_extension("manifest.json");
                    if manifest_path.is_file() {
                        artifacts.push(ArtifactEntry {
                            id: format_id(&rel_base),
                            output_base: output_dir.join(&rel_base),
                            kind: Artifact::NefManifest {
                                nef_path: path,
                                manifest_path,
                            },
                        });
                    } else {
                        eprintln!(
                            "Skipping {}: missing manifest {}",
                            path.display(),
                            manifest_path.display()
                        );
                    }
                }
                _ => {}
            }
        }
    }
    artifacts
}

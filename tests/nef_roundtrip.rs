//! Round-trip check of the NEF serializer against the real corpus.
//!
//! This is a development aid, not a unit test: it reads the pinned contract
//! corpus from disk and checks that parsing and re-serializing a container
//! reproduces the original bytes. The corpus is not version controlled, so
//! this cannot live in `tests/`; it runs as a test that skips when the
//! corpus is absent.
//!
//! Fixtures registered in `TestingArtifacts/expected_invalid.txt` are
//! intentionally malformed and are expected to be rejected by the parser, so
//! they are excluded from the check.

use neo_decompiler::nef::NefParser;

/// Collect the `.nef` files under a directory tree.
fn collect_nef(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_nef(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("nef") {
            out.push(path);
        }
    }
}

/// Load the ids of the fixtures that are intentionally malformed.
///
/// The registry lists an id as a path relative to `TestingArtifacts`, with an
/// optional `:expected error substring` suffix.
fn expected_invalid_ids(root: &std::path::Path) -> Vec<String> {
    let path = root.join("TestingArtifacts/expected_invalid.txt");
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let id = line.split(':').next().unwrap_or(line).trim();
            id.replace('\\', "/")
        })
        .collect()
}

/// The registry id of a corpus file: its path relative to `TestingArtifacts`,
/// with the `.nef` extension removed.
fn corpus_id(root: &std::path::Path, path: &std::path::Path) -> Option<String> {
    let artifacts = root.join("TestingArtifacts");
    let relative = path.strip_prefix(&artifacts).ok()?;
    let name = relative.to_string_lossy().replace('\\', "/");
    Some(name.strip_suffix(".nef").unwrap_or(&name).to_string())
}

#[test]
fn round_trips_the_real_contract_corpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let invalid_ids = expected_invalid_ids(root);

    let mut dirs = Vec::new();
    collect_nef(&root.join("TestingArtifacts/devpack"), &mut dirs);
    collect_nef(&root.join("TestingArtifacts/edgecases"), &mut dirs);
    dirs.sort();

    if dirs.is_empty() {
        eprintln!("no corpus on disk; skipping");
        return;
    }

    let mut checked = 0usize;
    let mut skipped = 0usize;
    let mut mismatched = Vec::new();
    for path in &dirs {
        // A fixture registered as intentionally malformed is expected to be
        // rejected, so there is nothing to round trip.
        if let Some(id) = corpus_id(root, path) {
            if invalid_ids.iter().any(|invalid| invalid == &id) {
                skipped += 1;
                continue;
            }
        }

        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(nef) = NefParser::new().parse(&bytes) else {
            mismatched.push(format!("{}: parse failed", path.display()));
            continue;
        };
        let encoded = nef.to_bytes();
        if encoded != bytes {
            mismatched.push(format!("{}: bytes differ", path.display()));
            continue;
        }
        // The re-serialized container must parse back to the same content.
        let reparsed = NefParser::new().parse(&encoded).expect("reparses");
        assert_eq!(reparsed.script, nef.script);
        assert_eq!(reparsed.method_tokens, nef.method_tokens);
        assert_eq!(reparsed.header.source, nef.header.source);
        assert_eq!(reparsed.header.compiler, nef.header.compiler);
        checked += 1;
    }

    eprintln!(
        "round-tripped {checked}/{} containers ({skipped} intentionally invalid)",
        dirs.len()
    );
    assert!(
        mismatched.is_empty(),
        "{} container(s) did not round trip:\n{}",
        mismatched.len(),
        mismatched.join("\n")
    );
    assert!(checked > 0, "no container was checked");
}

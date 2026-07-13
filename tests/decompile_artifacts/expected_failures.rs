use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExpectedFailure {
    pub(crate) id: String,
    pub(crate) expected: Option<String>,
}

fn parse_registry(contents: &str) -> Vec<ExpectedFailure> {
    let mut entries = contents
        .lines()
        .filter_map(|line| {
            let trimmed = line.split('#').next().map(str::trim).unwrap_or_default();
            if trimmed.is_empty() {
                return None;
            }
            let (id, expected) = if let Some((id, expected)) = trimmed.split_once(':') {
                (id.trim().to_string(), Some(expected.trim().to_string()))
            } else {
                (trimmed.to_string(), None)
            };
            Some(ExpectedFailure { id, expected })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries.dedup_by(|a, b| a.id == b.id && a.expected == b.expected);
    entries
}

fn load_registry(artifacts_dir: &Path, filename: &str) -> Vec<ExpectedFailure> {
    let path = artifacts_dir.join(filename);
    match fs::read_to_string(&path) {
        Ok(contents) => parse_registry(&contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => panic!("failed to read {}: {error}", path.display()),
    }
}

pub(crate) fn load_known_unsupported(artifacts_dir: &Path) -> Vec<ExpectedFailure> {
    load_registry(artifacts_dir, "known_unsupported.txt")
}

pub(crate) fn load_expected_invalid(artifacts_dir: &Path) -> Vec<ExpectedFailure> {
    load_registry(artifacts_dir, "expected_invalid.txt")
}

pub(crate) fn find_entry<'a>(
    id: &str,
    entries: &'a [ExpectedFailure],
) -> Option<&'a ExpectedFailure> {
    let basename = id.rsplit('/').next();
    entries
        .iter()
        .find(|entry| entry.id == id || basename.is_some_and(|name| name == entry.id))
}

pub(crate) fn find_expected_message<'a>(
    id: &str,
    entries: &'a [ExpectedFailure],
) -> Option<&'a str> {
    find_entry(id, entries).and_then(|entry| entry.expected.as_deref())
}

#[test]
fn registry_parser_sorts_deduplicates_and_preserves_expected_messages() {
    let parsed = parse_registry(
        "\n# comment\nz/path:expected text\na/path\nz/path:expected text # duplicate\n",
    );
    assert_eq!(
        parsed,
        vec![
            ExpectedFailure {
                id: "a/path".to_string(),
                expected: None,
            },
            ExpectedFailure {
                id: "z/path".to_string(),
                expected: Some("expected text".to_string()),
            },
        ]
    );
    let basename_only = [ExpectedFailure {
        id: "path".to_string(),
        expected: None,
    }];
    assert!(find_entry("parent/path", &basename_only).is_some());
    assert_eq!(
        find_expected_message("z/path", &parsed),
        Some("expected text")
    );
}

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnownUnsupported {
    pub(crate) id: String,
    pub(crate) expected: Option<String>,
}

pub(crate) fn load_known_unsupported(artifacts_dir: &Path) -> Vec<KnownUnsupported> {
    let mut entries = Vec::new();

    let path = artifacts_dir.join("known_unsupported.txt");
    if let Ok(contents) = fs::read_to_string(&path) {
        for line in contents.lines() {
            let trimmed = line.split('#').next().map(str::trim).unwrap_or_default();
            if trimmed.is_empty() {
                continue;
            }
            let (id, expected) = if let Some((id, expected)) = trimmed.split_once(':') {
                (id.trim().to_string(), Some(expected.trim().to_string()))
            } else {
                (trimmed.to_string(), None)
            };
            entries.push(KnownUnsupported { id, expected });
        }
    }

    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries.dedup_by(|a, b| a.id == b.id && a.expected == b.expected);
    entries
}

pub(crate) fn is_known_unsupported(id: &str, known_unsupported: &[KnownUnsupported]) -> bool {
    find_known_entry(id, known_unsupported).is_some()
}

pub(crate) fn find_known_entry<'a>(
    id: &str,
    known_unsupported: &'a [KnownUnsupported],
) -> Option<&'a KnownUnsupported> {
    let basename = id.rsplit('/').next();
    known_unsupported
        .iter()
        .find(|entry| entry.id == id || basename.map(|name| name == entry.id).unwrap_or(false))
}

pub(crate) fn find_expected_message<'a>(
    id: &str,
    known: &'a [KnownUnsupported],
) -> Option<&'a str> {
    find_known_entry(id, known).and_then(|entry| entry.expected.as_deref())
}

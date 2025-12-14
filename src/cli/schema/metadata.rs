use serde::Serialize;

use super::kind::SchemaKind;

#[derive(Clone, Copy)]
pub(in super::super) struct SchemaMetadata {
    pub(in super::super) kind: SchemaKind,
    pub(in super::super) version: &'static str,
    pub(in super::super) path: &'static str,
    pub(in super::super) contents: &'static str,
    pub(in super::super) description: &'static str,
}

impl SchemaMetadata {
    pub(super) const fn new(
        kind: SchemaKind,
        version: &'static str,
        path: &'static str,
        contents: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            kind,
            version,
            path,
            contents,
            description,
        }
    }

    pub(in super::super) fn report(&self) -> SchemaReport<'_> {
        SchemaReport {
            name: self.kind.as_str(),
            version: self.version,
            path: self.path,
            description: self.description,
        }
    }
}

#[derive(Serialize)]
pub(in super::super) struct SchemaReport<'a> {
    name: &'a str,
    version: &'a str,
    path: &'a str,
    description: &'a str,
}

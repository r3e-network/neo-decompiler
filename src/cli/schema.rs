use clap::ValueEnum;
use serde::Serialize;

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub(super) enum SchemaKind {
    Info,
    Disasm,
    Decompile,
    Tokens,
}

impl SchemaKind {
    pub(super) const ALL: [SchemaMetadata; 4] = [
        SchemaMetadata::new(
            SchemaKind::Info,
            SCHEMA_VERSION,
            SCHEMA_INFO_PATH,
            INFO_SCHEMA,
            "NEF metadata, manifest summary, method tokens, warnings",
        ),
        SchemaMetadata::new(
            SchemaKind::Disasm,
            SCHEMA_VERSION,
            SCHEMA_DISASM_PATH,
            DISASM_SCHEMA,
            "Instruction stream with operand metadata",
        ),
        SchemaMetadata::new(
            SchemaKind::Decompile,
            SCHEMA_VERSION,
            SCHEMA_DECOMPILE_PATH,
            DECOMPILE_SCHEMA,
            "High-level output + pseudocode + disassembly",
        ),
        SchemaMetadata::new(
            SchemaKind::Tokens,
            SCHEMA_VERSION,
            SCHEMA_TOKENS_PATH,
            TOKENS_SCHEMA,
            "Standalone method-token listing",
        ),
    ];

    pub(super) fn as_str(self) -> &'static str {
        match self {
            SchemaKind::Info => "info",
            SchemaKind::Disasm => "disasm",
            SchemaKind::Decompile => "decompile",
            SchemaKind::Tokens => "tokens",
        }
    }

    pub(super) fn metadata(self) -> SchemaMetadata {
        match self {
            SchemaKind::Info => Self::ALL[0],
            SchemaKind::Disasm => Self::ALL[1],
            SchemaKind::Decompile => Self::ALL[2],
            SchemaKind::Tokens => Self::ALL[3],
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct SchemaMetadata {
    pub(super) kind: SchemaKind,
    pub(super) version: &'static str,
    pub(super) path: &'static str,
    pub(super) contents: &'static str,
    pub(super) description: &'static str,
}

impl SchemaMetadata {
    const fn new(
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

    pub(super) fn report(&self) -> SchemaReport<'_> {
        SchemaReport {
            name: self.kind.as_str(),
            version: self.version,
            path: self.path,
            description: self.description,
        }
    }
}

#[derive(Serialize)]
pub(super) struct SchemaReport<'a> {
    name: &'a str,
    version: &'a str,
    path: &'a str,
    description: &'a str,
}

const SCHEMA_VERSION: &str = "1.0.0";

const SCHEMA_INFO_PATH: &str = "docs/schema/info.schema.json";
const SCHEMA_DISASM_PATH: &str = "docs/schema/disasm.schema.json";
const SCHEMA_DECOMPILE_PATH: &str = "docs/schema/decompile.schema.json";
const SCHEMA_TOKENS_PATH: &str = "docs/schema/tokens.schema.json";

const INFO_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schema/info.schema.json"
));
const DISASM_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schema/disasm.schema.json"
));
const DECOMPILE_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schema/decompile.schema.json"
));
const TOKENS_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schema/tokens.schema.json"
));

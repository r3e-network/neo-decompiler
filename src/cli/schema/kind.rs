use clap::ValueEnum;

use super::embedded::{
    DECOMPILE_SCHEMA, DISASM_SCHEMA, INFO_SCHEMA, SCHEMA_DECOMPILE_PATH, SCHEMA_DISASM_PATH,
    SCHEMA_INFO_PATH, SCHEMA_TOKENS_PATH, SCHEMA_VERSION, TOKENS_SCHEMA,
};
use super::metadata::SchemaMetadata;

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub(in super::super) enum SchemaKind {
    Info,
    Disasm,
    Decompile,
    Tokens,
}

impl SchemaKind {
    pub(in super::super) const ALL: [SchemaMetadata; 4] = [
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
            "High-level output + pseudocode + disassembly + analysis",
        ),
        SchemaMetadata::new(
            SchemaKind::Tokens,
            SCHEMA_VERSION,
            SCHEMA_TOKENS_PATH,
            TOKENS_SCHEMA,
            "Standalone method-token listing",
        ),
    ];

    pub(in super::super) const fn as_str(self) -> &'static str {
        match self {
            SchemaKind::Info => "info",
            SchemaKind::Disasm => "disasm",
            SchemaKind::Decompile => "decompile",
            SchemaKind::Tokens => "tokens",
        }
    }

    pub(in super::super) const fn metadata(self) -> SchemaMetadata {
        match self {
            SchemaKind::Info => Self::ALL[0],
            SchemaKind::Disasm => Self::ALL[1],
            SchemaKind::Decompile => Self::ALL[2],
            SchemaKind::Tokens => Self::ALL[3],
        }
    }
}

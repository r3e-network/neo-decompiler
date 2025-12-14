use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use jsonschema::JSONSchema;
use serde_json::Value;
use std::fmt::Write;
use std::path::Path;
use std::sync::OnceLock;

pub(crate) fn neo_decompiler_cmd() -> Command {
    cargo_bin_cmd!("neo-decompiler")
}

pub(crate) const GAS_TOKEN_HASH: [u8; 20] = [
    0xCF, 0x76, 0xE2, 0x8B, 0xD0, 0x06, 0x2C, 0x4A, 0x47, 0x8E, 0xE3, 0x55, 0x61, 0x01, 0x13, 0x19,
    0xF3, 0xCF, 0xA4, 0xD2,
];

fn write_varint(buf: &mut Vec<u8>, value: u32) {
    match value {
        0x00..=0xFC => buf.push(value as u8),
        0xFD..=0xFFFF => {
            buf.push(0xFD);
            buf.extend_from_slice(&(value as u16).to_le_bytes());
        }
        _ => {
            buf.push(0xFE);
            buf.extend_from_slice(&value.to_le_bytes());
        }
    }
}

pub(crate) fn build_sample_nef() -> Vec<u8> {
    let script = [0x10, 0x11, 0x9E, 0x40];
    let mut data = Vec::new();
    data.extend_from_slice(b"NEF3");
    let mut compiler = [0u8; 64];
    compiler[..4].copy_from_slice(b"test");
    data.extend_from_slice(&compiler);
    data.push(0); // source (empty)
    data.push(0); // reserved byte
                  // single method token
    data.push(1);
    data.extend_from_slice(&GAS_TOKEN_HASH);
    write_varint(&mut data, 8);
    data.extend_from_slice(b"Transfer");
    data.extend_from_slice(&2u16.to_le_bytes()); // params
    data.push(1); // has return value
    data.push(0x0F); // call flags (CallFlags::All)
    data.extend_from_slice(&0u16.to_le_bytes()); // reserved word
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = neo_decompiler::nef::NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());
    data
}

pub(crate) fn build_nef_with_no_tokens() -> Vec<u8> {
    let script = [0x40];
    let mut data = Vec::new();
    data.extend_from_slice(b"NEF3");
    let mut compiler = [0u8; 64];
    compiler[..4].copy_from_slice(b"test");
    data.extend_from_slice(&compiler);
    data.push(0); // source
    data.push(0); // reserved byte
    data.push(0); // zero tokens
    data.extend_from_slice(&0u16.to_le_bytes()); // reserved word
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = neo_decompiler::nef::NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());
    data
}

pub(crate) fn build_nef_with_unknown_opcode() -> Vec<u8> {
    let script = [0xFF, 0x40]; // UNKNOWN, RET
    let mut data = Vec::new();
    data.extend_from_slice(b"NEF3");
    let mut compiler = [0u8; 64];
    compiler[..4].copy_from_slice(b"test");
    data.extend_from_slice(&compiler);
    data.push(0); // source
    data.push(0); // reserved byte
    data.push(0); // zero tokens
    data.extend_from_slice(&0u16.to_le_bytes()); // reserved word
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = neo_decompiler::nef::NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());
    data
}

pub(crate) const SAMPLE_MANIFEST: &str = r#"
{
    "name": "SampleToken",
    "groups": [
        {
            "pubkey": "039999999999999999999999999999999999999999999999999999999999999999",
            "signature": "deadbeef"
        }
    ],
    "supportedstandards": ["NEP-17"],
    "features": { "storage": true, "payable": false },
    "abi": {
        "methods": [
            {
                "name": "symbol",
                "parameters": [],
                "returntype": "String",
                "offset": 0,
                "safe": true
            }
        ],
        "events": []
    },
    "permissions": [
        {
            "contract": { "hash": "0x0123456789ABCDEFFEDCBA987654321001234567" },
            "methods": ["symbol"]
        },
        {
            "contract": { "group": "03ABCD" },
            "methods": "*"
        }
    ],
    "trusts": ["0x89ABCDEF0123456789ABCDEF0123456789ABCDEF"],
    "extra": {
        "Author": "Example Author",
        "Email": "author@example.com"
    }
}
"#;

#[derive(Debug, Clone, Copy)]
pub(crate) enum SchemaKind {
    Info,
    Disasm,
    Decompile,
    Tokens,
}

pub(crate) fn assert_schema(kind: SchemaKind, payload: &Value) {
    if let Err(errors) = schema(kind).validate(payload) {
        let mut message = String::new();
        let _ = writeln!(&mut message, "Schema validation failed for {kind:?}:");
        for error in errors {
            let _ = writeln!(&mut message, "- {error}");
        }
        panic!("{message}");
    }
}

fn schema(kind: SchemaKind) -> &'static JSONSchema {
    static INFO_SCHEMA: OnceLock<JSONSchema> = OnceLock::new();
    static DISASM_SCHEMA: OnceLock<JSONSchema> = OnceLock::new();
    static DECOMPILE_SCHEMA: OnceLock<JSONSchema> = OnceLock::new();
    static TOKENS_SCHEMA: OnceLock<JSONSchema> = OnceLock::new();

    match kind {
        SchemaKind::Info => INFO_SCHEMA.get_or_init(|| load_schema("info")),
        SchemaKind::Disasm => DISASM_SCHEMA.get_or_init(|| load_schema("disasm")),
        SchemaKind::Decompile => DECOMPILE_SCHEMA.get_or_init(|| load_schema("decompile")),
        SchemaKind::Tokens => TOKENS_SCHEMA.get_or_init(|| load_schema("tokens")),
    }
}

fn load_schema(name: &str) -> JSONSchema {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir
        .join("docs")
        .join("schema")
        .join(format!("{name}.schema.json"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read schema {}: {err}", path.display()));
    let schema_json: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|err| panic!("failed to parse schema {}: {err}", path.display()));
    JSONSchema::compile(&schema_json)
        .unwrap_or_else(|err| panic!("failed to compile schema {}: {err}", path.display()))
}

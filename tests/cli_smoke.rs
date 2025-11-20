use assert_cmd::Command;
use jsonschema::JSONSchema;
use predicates::str::contains;
use serde_json::Value;
use std::fmt::Write;
use std::path::Path;
use std::sync::OnceLock;
use tempfile::tempdir;

const GAS_TOKEN_HASH: [u8; 20] = [
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

fn build_sample_nef() -> Vec<u8> {
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

fn build_nef_with_no_tokens() -> Vec<u8> {
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

fn build_nef_with_unknown_opcode() -> Vec<u8> {
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

#[test]
fn info_command_prints_header() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("info")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("Method tokens: 1"))
        .stdout(contains("#0: hash="))
        .stdout(contains("(GasToken::Transfer)"))
        .stdout(contains(
            "Script hash (LE): 9DE87DC65A6A581E502CAE845C6F13645B10C5EA",
        ))
        .stdout(contains(
            "Script hash (BE): EAC5105B64136F5C84AE2C501E586A5AC67DE89D",
        ))
        .stdout(contains(
            "flags=0x0F (ReadStates|WriteStates|AllowCall|AllowNotify)",
        ));
}

#[test]
fn info_command_supports_json_output() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    let manifest_path = dir.path().join("contract.manifest.json");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();
    std::fs::write(&manifest_path, SAMPLE_MANIFEST).unwrap();

    let output = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("info")
        .arg("--format")
        .arg("json")
        .arg(&nef_path)
        .output()
        .expect("json output");

    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).expect("valid json");
    assert_eq!(
        value["script_hash_le"],
        Value::String("9DE87DC65A6A581E502CAE845C6F13645B10C5EA".into())
    );
    let tokens = value["method_tokens"].as_array().expect("tokens array");
    assert!(!tokens.is_empty());
    assert_eq!(
        value["manifest"]["abi"]["methods"][0]["name"],
        Value::String("symbol".into())
    );
    assert_eq!(
        value["manifest"]["permissions"][0]["contract"]["type"],
        Value::String("Hash".into())
    );
    assert_eq!(
        value["manifest"]["trusts"]["type"],
        Value::String("Contracts".into())
    );
    assert_eq!(
        value["manifest_path"],
        Value::String(manifest_path.display().to_string())
    );
    assert!(value["warnings"].is_array());
    assert_eq!(
        value["manifest"]["permissions"][1]["contract"]["type"],
        Value::String("Group".into())
    );
    assert_eq!(
        value["manifest"]["permissions"][1]["contract"]["value"],
        Value::String("03ABCD".into())
    );
    assert_eq!(
        value["manifest"]["groups"][0]["pubkey"],
        Value::String("039999999999999999999999999999999999999999999999999999999999999999".into())
    );
    assert_eq!(
        value["manifest"]["groups"][0]["signature"],
        Value::String("deadbeef".into())
    );
    assert_eq!(tokens[0]["returns_value"], Value::Bool(true));
    assert_schema(SchemaKind::Info, &value);

    let compact = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("info")
        .arg("--format")
        .arg("json")
        .arg("--json-compact")
        .arg(&nef_path)
        .output()
        .expect("compact json output");
    assert!(compact.status.success());
    assert!(
        compact.stdout.len() < output.stdout.len(),
        "compact json should be shorter"
    );
    let compact_value: Value = serde_json::from_slice(&compact.stdout).expect("compact json parse");
    assert_eq!(value, compact_value);
}

#[test]
fn disasm_command_outputs_instructions() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    let output = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("disasm")
        .arg(&nef_path)
        .output()
        .expect("disasm output");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("0000: PUSH0"));

    let json_output = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("disasm")
        .arg("--format")
        .arg("json")
        .arg(&nef_path)
        .output()
        .expect("json disasm");
    assert!(json_output.status.success());
    let value: Value = serde_json::from_slice(&json_output.stdout).expect("json parse");
    let instructions = value["instructions"].as_array().expect("array");
    assert_eq!(instructions[0]["opcode"], "PUSH0");
    assert_eq!(instructions[0]["offset"], 0);
    assert_eq!(instructions[0]["operand_kind"], Value::String("I32".into()));
    assert_eq!(
        instructions[0]["operand_value"]["type"],
        Value::String("I32".into())
    );
    assert_eq!(instructions[0]["operand_value"]["value"], Value::from(0));
    assert_eq!(instructions[1]["operand_kind"], Value::String("I32".into()));
    assert_eq!(instructions[1]["operand_value"]["value"], Value::from(1));
    assert!(value["warnings"].is_array());
    assert_schema(SchemaKind::Disasm, &value);
}

#[test]
fn disasm_can_allow_unknown_opcodes() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("unknown.nef");
    std::fs::write(&nef_path, build_nef_with_unknown_opcode()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("disasm")
        .arg(&nef_path)
        .assert()
        .failure()
        .stderr(contains("unknown opcode 0xFF"));

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("disasm")
        .arg("--allow-unknown-opcodes")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("UNKNOWN_0xFF"))
        .stdout(contains("0001: RET"));
}

#[test]
fn decompile_command_outputs_high_level_by_default() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("decompile")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("contract NeoContract"))
        .stdout(contains("GasToken::Transfer"));
}

#[test]
fn decompile_command_supports_pseudocode_format() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("decompile")
        .arg("--format")
        .arg("pseudocode")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("ADD"));
}

#[test]
fn decompile_command_supports_csharp_format() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    let manifest_path = dir.path().join("contract.manifest.json");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();
    std::fs::write(&manifest_path, SAMPLE_MANIFEST).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("decompile")
        .arg("--format")
        .arg("csharp")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("namespace NeoDecompiler.Generated"))
        .stdout(contains("public static string symbol()"));
}

#[test]
fn decompile_command_supports_json_format() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    let manifest_path = dir.path().join("contract.manifest.json");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();
    std::fs::write(&manifest_path, SAMPLE_MANIFEST).unwrap();

    let output = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("decompile")
        .arg("--format")
        .arg("json")
        .arg(&nef_path)
        .output()
        .expect("json decompile");
    assert!(output.status.success());

    let value: Value = serde_json::from_slice(&output.stdout).expect("json parse");
    assert!(value["high_level"]
        .as_str()
        .expect("string")
        .contains("contract SampleToken"));
    assert_eq!(
        value["manifest_path"],
        Value::String(manifest_path.display().to_string())
    );
    assert!(value["csharp"]
        .as_str()
        .expect("csharp string")
        .contains("namespace NeoDecompiler.Generated"));
    assert!(value["csharp"]
        .as_str()
        .expect("csharp string")
        .contains("[ManifestExtra(\"Author\", \"Example Author\")]"));
    assert_eq!(
        value["manifest"]["abi"]["methods"][0]["name"],
        Value::String("symbol".into())
    );
    assert_eq!(
        value["instructions"][0]["opcode"],
        Value::String("PUSH0".into())
    );
    assert_eq!(
        value["instructions"][0]["operand_value"]["value"],
        Value::from(0)
    );
    assert_eq!(
        value["manifest"]["permissions"][0]["contract"]["type"],
        Value::String("Hash".into())
    );
    assert_eq!(
        value["manifest"]["trusts"]["type"],
        Value::String("Contracts".into())
    );
    assert!(value["warnings"].is_array());
    let tokens = value["method_tokens"].as_array().expect("tokens array");
    assert_eq!(tokens[0]["returns_value"], Value::Bool(true));
    assert_eq!(
        value["manifest"]["groups"][0]["pubkey"],
        Value::String("039999999999999999999999999999999999999999999999999999999999999999".into())
    );
    assert_schema(SchemaKind::Decompile, &value);
}

#[test]
fn catalog_command_lists_syscalls() {
    let output = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("catalog")
        .arg("syscalls")
        .output()
        .expect("catalog syscalls");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("System.Runtime.Platform"));
    assert!(stdout.contains("call_flags"));
    assert!(stdout.contains("returns_value"));
}

#[test]
fn catalog_command_supports_json_output() {
    let output = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("catalog")
        .arg("native-contracts")
        .arg("--format")
        .arg("json")
        .output()
        .expect("catalog native contracts json");
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).expect("json parse");
    assert_eq!(value["kind"], Value::String("native-contracts".into()));
    assert!(value["count"].as_u64().unwrap_or(0) > 0);
    let entries = value["entries"].as_array().expect("entries array");
    assert!(!entries.is_empty());
    assert!(entries[0]["methods"].is_array());
}

#[test]
fn decompile_command_uses_manifest_when_provided() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    let manifest_path = dir.path().join("custom.manifest.json");

    std::fs::write(&nef_path, build_sample_nef()).unwrap();
    std::fs::write(&manifest_path, SAMPLE_MANIFEST).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("decompile")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("contract SampleToken"))
        .stdout(contains("permissions {"))
        .stdout(contains("trusts = ["));
}

#[test]
fn tokens_command_lists_entries() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("tokens")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("method=Transfer"))
        .stdout(contains("GasToken::Transfer"))
        .stdout(contains("AllowCall"));
}

#[test]
fn tokens_command_supports_json_output() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    let output = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("tokens")
        .arg("--format")
        .arg("json")
        .arg(&nef_path)
        .output()
        .expect("json output");
    assert!(output.status.success());

    let value: Value = serde_json::from_slice(&output.stdout).expect("json parse");
    let tokens = value["method_tokens"].as_array().expect("array");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0]["native_contract"]["label"], "GasToken::Transfer");
    assert!(value["warnings"].is_array());
    assert_schema(SchemaKind::Tokens, &value);
}

#[test]
fn tokens_command_handles_empty() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_nef_with_no_tokens()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("tokens")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("no method tokens"));
}

#[test]
fn info_command_loads_manifest_when_available() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    let manifest_path = dir.path().join("contract.manifest.json");

    std::fs::write(&nef_path, build_sample_nef()).unwrap();
    std::fs::write(&manifest_path, SAMPLE_MANIFEST).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("info")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("Manifest contract: SampleToken"))
        .stdout(contains("Groups:"))
        .stdout(contains(
            "pubkey=039999999999999999999999999999999999999999999999999999999999999999",
        ))
        .stdout(contains("Permissions:"))
        .stdout(contains("Trusts:"));
}

#[test]
fn schema_command_outputs_embedded_schema() {
    let pretty = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("schema")
        .arg("info")
        .output()
        .expect("schema command");
    assert!(pretty.status.success());
    let schema: Value = serde_json::from_slice(&pretty.stdout).expect("valid schema json");
    assert_eq!(
        schema["title"],
        Value::String("neo-decompiler info report".into())
    );

    let compact = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("--json-compact")
        .arg("schema")
        .arg("info")
        .output()
        .expect("compact schema command");
    assert!(compact.status.success());
    assert!(compact.stdout.len() < pretty.stdout.len());

    let list = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("schema")
        .arg("--list")
        .output()
        .expect("list schemas");
    assert!(list.status.success());
    let listing = String::from_utf8_lossy(&list.stdout);
    assert!(listing.contains("info v1.0.0 -"));
    assert!(listing.contains("disasm v1.0.0 -"));

    let json_list = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("schema")
        .arg("--list-json")
        .output()
        .expect("json schema list");
    assert!(json_list.status.success());
    let entries: Value = serde_json::from_slice(&json_list.stdout).expect("json list");
    assert!(entries.is_array());
    assert_eq!(entries[0]["name"], Value::String("info".into()));
    assert_eq!(entries[0]["version"], Value::String("1.0.0".into()));
    assert_eq!(
        entries[0]["path"],
        Value::String("docs/schema/info.schema.json".into())
    );

    let dir = tempdir().expect("schema dir");
    let schema_path = dir.path().join("info.schema.json");
    let file_output = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("schema")
        .arg("info")
        .arg("--json-compact")
        .arg("--output")
        .arg(&schema_path)
        .output()
        .expect("schema output to file");
    assert!(file_output.status.success());
    let file_contents = std::fs::read_to_string(&schema_path).expect("schema written to disk");
    let persisted: Value = serde_json::from_str(&file_contents).expect("file schema valid");
    assert_eq!(
        persisted["title"],
        Value::String("neo-decompiler info report".into())
    );

    let quiet_schema_path = dir.path().join("info-quiet.schema.json");
    let quiet_output = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("schema")
        .arg("info")
        .arg("--json-compact")
        .arg("--output")
        .arg(&quiet_schema_path)
        .arg("--quiet")
        .output()
        .expect("schema output to file (quiet)");
    assert!(quiet_output.status.success());
    assert!(
        quiet_output.stdout.is_empty(),
        "quiet schema output should suppress stdout"
    );
    let quiet_contents =
        std::fs::read_to_string(&quiet_schema_path).expect("quiet schema written to disk");
    let quiet_value: Value = serde_json::from_str(&quiet_contents).expect("quiet schema valid");
    assert_eq!(
        quiet_value["title"],
        Value::String("neo-decompiler info report".into())
    );

    let nef_path = dir.path().join("validation.nef");
    let manifest_path = dir.path().join("validation.manifest.json");
    let info_json_path = dir.path().join("info.json");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();
    std::fs::write(&manifest_path, SAMPLE_MANIFEST).unwrap();
    let info_json = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("info")
        .arg("--format")
        .arg("json")
        .arg(&nef_path)
        .output()
        .expect("info json output");
    assert!(info_json.status.success());
    let info_json_bytes = info_json.stdout.clone();
    std::fs::write(&info_json_path, &info_json_bytes).unwrap();

    let validation = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("schema")
        .arg("info")
        .arg("--validate")
        .arg(&info_json_path)
        .output()
        .expect("schema validation");
    if !validation.status.success() {
        panic!(
            "schema validation stderr: {}",
            String::from_utf8_lossy(&validation.stderr)
        );
    }
    assert!(String::from_utf8_lossy(&validation.stdout).contains("Validation succeeded"));

    let stdin_validation = Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("schema")
        .arg("info")
        .arg("--validate")
        .arg("-")
        .arg("--no-print")
        .write_stdin(info_json_bytes.clone())
        .output()
        .expect("schema validation via stdin");
    if !stdin_validation.status.success() {
        panic!(
            "schema validation (stdin) stderr: {}",
            String::from_utf8_lossy(&stdin_validation.stderr)
        );
    }
    assert!(String::from_utf8_lossy(&stdin_validation.stdout).contains("Validation succeeded"));
}

const SAMPLE_MANIFEST: &str = r#"
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
enum SchemaKind {
    Info,
    Disasm,
    Decompile,
    Tokens,
}

fn assert_schema(kind: SchemaKind, payload: &Value) {
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

use super::csharp::csharpize_statement;
use super::helpers::sanitize_identifier;
use super::high_level::HighLevelEmitter;
use super::*;
use crate::disassembler::UnknownHandling;
use crate::{ContractManifest, NefParser};
use std::{fs, path::PathBuf};

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

fn build_nef(script: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"NEF3");
    let mut compiler = [0u8; 64];
    compiler[..4].copy_from_slice(b"test");
    data.extend_from_slice(&compiler);
    data.push(0); // source (empty)
    data.push(0); // reserved byte
    data.push(0); // method token count
    data.extend_from_slice(&0u16.to_le_bytes()); // reserved word
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());
    data
}

fn build_nef_with_single_token(
    script: &[u8],
    hash: [u8; 20],
    method: &str,
    parameters_count: u16,
    has_return_value: bool,
    call_flags: u8,
) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"NEF3");
    let mut compiler = [0u8; 64];
    compiler[..4].copy_from_slice(b"test");
    data.extend_from_slice(&compiler);
    data.push(0); // source (empty)
    data.push(0); // reserved byte

    data.push(1); // method token count
    data.extend_from_slice(&hash);
    write_varint(&mut data, method.len() as u32);
    data.extend_from_slice(method.as_bytes());
    data.extend_from_slice(&parameters_count.to_le_bytes());
    data.push(u8::from(has_return_value));
    data.push(call_flags);

    data.extend_from_slice(&0u16.to_le_bytes()); // reserved word
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());
    data
}

fn sample_nef() -> Vec<u8> {
    // Build a minimal NEF with script: PUSH0, PUSH1, ADD, RET
    build_nef(&[0x10, 0x11, 0x9E, 0x40])
}

fn sample_manifest() -> ContractManifest {
    ContractManifest::from_json_str(
        r#"
            {
                "name": "ExampleContract",
                "supportedstandards": ["NEP-17"],
                "features": {"storage": true, "payable": false},
                "abi": {
                    "methods": [
                        {
                            "name": "main",
                            "parameters": [],
                            "returntype": "Integer",
                            "offset": 0,
                            "safe": false
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*"
            }
            "#,
    )
    .expect("manifest parsed")
}

fn testing_artifact_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("TestingArtifacts")
        .join("devpack")
}

fn load_testing_nef(name: &str) -> Vec<u8> {
    fs::read(testing_artifact_dir().join(name)).expect("failed to read NEF artifact")
}

fn load_testing_manifest(name: &str) -> ContractManifest {
    let path = testing_artifact_dir().join(name);
    let data = fs::read_to_string(&path).expect("failed to read manifest artifact");
    ContractManifest::from_json_str(&data).expect("manifest parsed")
}

mod core;
mod csharp;
mod high_level;

#[test]
#[ignore]
fn debug_inferred_method_starts_contract_delegate() {
    let nef_bytes = load_testing_nef("Contract_Delegate.nef");
    let manifest = load_testing_manifest("Contract_Delegate.manifest.json");
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest.clone()), OutputFormat::All)
        .expect("decompile succeeds");
    let inferred = crate::decompiler::helpers::inferred_method_starts(
        &decompilation.instructions,
        decompilation.manifest.as_ref(),
    );
    eprintln!("inferred starts: {inferred:?}");
}

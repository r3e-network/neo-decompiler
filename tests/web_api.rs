#![cfg(feature = "web")]

use serde_json::Value;

const GAS_TOKEN_HASH: [u8; 20] = [
    0xCF, 0x76, 0xE2, 0x8B, 0xD0, 0x06, 0x2C, 0x4A, 0x47, 0x8E, 0xE3, 0x55, 0x61, 0x01, 0x13, 0x19,
    0xF3, 0xCF, 0xA4, 0xD2,
];

const SAMPLE_MANIFEST: &str = r#"
{
    "name": "SampleToken",
    "groups": [],
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
    "permissions": [],
    "trusts": [],
    "extra": {}
}
"#;

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
    data.push(0);
    data.push(0);
    data.push(1);
    data.extend_from_slice(&GAS_TOKEN_HASH);
    write_varint(&mut data, 8);
    data.extend_from_slice(b"Transfer");
    data.extend_from_slice(&2u16.to_le_bytes());
    data.push(1);
    data.push(0x0F);
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = neo_decompiler::nef::NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());
    data
}

fn build_nef_with_unknown_opcode() -> Vec<u8> {
    let script = [0xFF, 0x40];
    let mut data = Vec::new();
    data.extend_from_slice(b"NEF3");
    let mut compiler = [0u8; 64];
    compiler[..4].copy_from_slice(b"test");
    data.extend_from_slice(&compiler);
    data.push(0);
    data.push(0);
    data.push(0);
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = neo_decompiler::nef::NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());
    data
}

#[test]
fn web_info_report_exposes_hashes_and_manifest_summary() {
    let report = neo_decompiler::web::info_report(&build_sample_nef(), Some(SAMPLE_MANIFEST))
        .expect("info report");

    let value = serde_json::to_value(&report).expect("serializable");
    assert_eq!(value["compiler"], Value::String("test".to_string()));
    assert_eq!(value["script_length"], Value::from(4));
    assert_eq!(
        value["manifest"]["name"],
        Value::String("SampleToken".to_string())
    );
    assert_eq!(
        value["method_tokens"][0]["method"],
        Value::String("Transfer".to_string())
    );
}

#[test]
fn web_disasm_report_surfaces_unknown_opcode_warnings() {
    let report = neo_decompiler::web::disasm_report(
        &build_nef_with_unknown_opcode(),
        neo_decompiler::web::WebDisasmOptions {
            fail_on_unknown_opcodes: false,
        },
    )
    .expect("disasm report");

    let value = serde_json::to_value(&report).expect("serializable");
    assert_eq!(
        value["instructions"][0]["opcode"],
        Value::String("UNKNOWN".to_string())
    );
    assert!(!value["warnings"].as_array().expect("warnings").is_empty());
}

#[test]
fn web_decompile_report_exposes_high_level_and_csharp_outputs() {
    let report = neo_decompiler::web::decompile_report(
        &build_sample_nef(),
        neo_decompiler::web::WebDecompileOptions {
            manifest_json: Some(SAMPLE_MANIFEST.to_string()),
            strict_manifest: false,
            fail_on_unknown_opcodes: false,
            inline_single_use_temps: false,
            output_format: neo_decompiler::OutputFormat::All,
        },
    )
    .expect("decompile report");

    let value = serde_json::to_value(&report).expect("serializable");
    assert!(value["high_level"]
        .as_str()
        .expect("high level")
        .contains("contract"));
    assert!(value["csharp"]
        .as_str()
        .expect("csharp")
        .contains("public class"));
    assert!(value["analysis"]["call_graph"]["methods"].is_array());
}

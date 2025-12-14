use serde_json::Value;
use tempfile::tempdir;

use crate::common::{build_sample_nef, neo_decompiler_cmd, SAMPLE_MANIFEST};

#[test]
fn schema_command_outputs_embedded_schema() {
    let pretty = neo_decompiler_cmd()
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

    let compact = neo_decompiler_cmd()
        .arg("--json-compact")
        .arg("schema")
        .arg("info")
        .output()
        .expect("compact schema command");
    assert!(compact.status.success());
    assert!(compact.stdout.len() < pretty.stdout.len());

    let list = neo_decompiler_cmd()
        .arg("schema")
        .arg("--list")
        .output()
        .expect("list schemas");
    assert!(list.status.success());
    let listing = String::from_utf8_lossy(&list.stdout);
    assert!(listing.contains("info v1.1.0 -"));
    assert!(listing.contains("disasm v1.1.0 -"));

    let json_list = neo_decompiler_cmd()
        .arg("schema")
        .arg("--list-json")
        .output()
        .expect("json schema list");
    assert!(json_list.status.success());
    let entries: Value = serde_json::from_slice(&json_list.stdout).expect("json list");
    assert!(entries.is_array());
    assert_eq!(entries[0]["name"], Value::String("info".into()));
    assert_eq!(entries[0]["version"], Value::String("1.1.0".into()));
    assert_eq!(
        entries[0]["path"],
        Value::String("docs/schema/info.schema.json".into())
    );

    let dir = tempdir().expect("schema dir");
    let schema_path = dir.path().join("info.schema.json");
    let file_output = neo_decompiler_cmd()
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
    let quiet_output = neo_decompiler_cmd()
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
    let info_json = neo_decompiler_cmd()
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

    let validation = neo_decompiler_cmd()
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

    let stdin_validation = neo_decompiler_cmd()
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

use super::*;

#[test]
fn decompile_end_to_end() {
    let nef_bytes = sample_nef();
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    assert_eq!(decompilation.instructions.len(), 4);
    assert!(decompilation
        .pseudocode
        .as_deref()
        .expect("pseudocode output")
        .contains("ADD"));
    assert!(decompilation
        .high_level
        .as_deref()
        .expect("high-level output")
        .contains("contract NeoContract"));
    assert!(decompilation
        .high_level
        .as_deref()
        .expect("high-level output")
        .contains("fn script_entry()"));
}

#[test]
fn decompile_with_manifest_produces_contract_name() {
    let nef_bytes = sample_nef();
    let manifest = sample_manifest();
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds with manifest");

    assert!(decompilation
        .high_level
        .as_deref()
        .expect("high-level output")
        .contains("contract ExampleContract"));
    assert!(decompilation
        .high_level
        .as_deref()
        .expect("high-level output")
        .contains("fn main() -> int {"));
}

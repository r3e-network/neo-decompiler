use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use neo_decompiler::{ContractManifest, Decompiler, OutputFormat};

fn private_method_block<'a>(text: &'a str, name: &str, next_name: &str) -> &'a str {
    let start = [
        "bool",
        "dynamic",
        "object[]",
        "ByteString",
        "BigInteger",
        "void",
    ]
    .iter()
    .find_map(|return_type| {
        let marker = format!("private static {return_type} {name}(");
        text.find(&marker)
    })
    .unwrap_or_else(|| panic!("missing private method `{name}`"));
    let end = [
        "bool",
        "dynamic",
        "object[]",
        "ByteString",
        "BigInteger",
        "void",
    ]
    .iter()
    .find_map(|return_type| {
        let marker = format!("private static {return_type} {next_name}(");
        text[start..].find(&marker).map(|relative| start + relative)
    })
    .unwrap_or(text.len());
    &text[start..end]
}

fn corpus_root() -> PathBuf {
    env::var_os("NEO_CSHARP_CORPUS_DIR").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts/devpack"),
        PathBuf::from,
    )
}

fn decompile_csharp(root: &Path, contract: &str) -> Option<String> {
    let nef_path = root.join(format!("{contract}.nef"));
    let manifest_path = root.join(format!("{contract}.manifest.json"));
    if !nef_path.is_file() || !manifest_path.is_file() {
        return None;
    }
    let nef = fs::read(&nef_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", nef_path.display()));
    let manifest_text = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
    let manifest = ContractManifest::from_json_str(&manifest_text)
        .unwrap_or_else(|error| panic!("invalid manifest {}: {error}", manifest_path.display()));
    Some(
        Decompiler::new()
            .with_trace_comments(false)
            .with_typed_declarations(true)
            .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::CSharp)
            .unwrap_or_else(|error| panic!("failed to decompile {}: {error}", nef_path.display()))
            .csharp
            .expect("C# output"),
    )
}

#[test]
fn lambda_and_linq_scan_helpers_use_csharp_for_loops() {
    let root = corpus_root();
    let Some(lambda) = decompile_csharp(&root, "Contract_Lambda") else {
        eprintln!("Skipping C# loop parity: Contract_Lambda artifacts are unavailable");
        return;
    };
    let lambda_scan = private_method_block(&lambda, "sub_0x022F", "sub_0x024F");
    assert!(
        lambda_scan.contains("for ("),
        "Lambda predicate scan should use a C# for loop: {lambda_scan}"
    );
    assert!(
        !lambda_scan.contains("while ("),
        "Lambda predicate scan should not retain a while loop: {lambda_scan}"
    );
    assert!(
        lambda_scan.contains("return true;") && lambda_scan.contains("if ("),
        "Lambda scan must retain its terminal predicate return: {lambda_scan}"
    );

    let Some(linq) = decompile_csharp(&root, "Contract_Linq") else {
        eprintln!("Skipping C# loop parity: Contract_Linq artifacts are unavailable");
        return;
    };
    for (start, next) in [
        ("sub_0x000D", "sub_0x0037"),
        ("sub_0x009B", "sub_0x00CC"),
        ("sub_0x0134", "sub_0x0168"),
        ("sub_0x025C", "sub_0x0301"),
    ] {
        let scan = private_method_block(&linq, start, next);
        assert!(
            scan.contains("for (") && !scan.contains("while ("),
            "Linq scan helper should use a C# for loop: {scan}"
        );
    }
}

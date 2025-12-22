use super::*;

#[test]
fn csharp_view_respects_manifest_metadata_and_parameters() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "Demo",
                "abi": {
                    "methods": [
                        {
                            "name": "deploy-contract",
                            "parameters": [
                                {"name": "owner-name", "type": "Hash160"},
                                {"name": "amount", "type": "Integer"}
                            ],
                            "returntype": "Void",
                            "offset": 0
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*",
                "extra": {"Author": "Jane Doe", "Email": "jane@example.com"}
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(csharp.contains("[ManifestExtra(\"Author\", \"Jane Doe\")]"));
    assert!(csharp.contains("[ManifestExtra(\"Email\", \"jane@example.com\")]"));
    assert!(csharp
        .contains("public static void deploy_contract(UInt160 owner_name, BigInteger amount)"));
}

#[test]
fn csharpize_statement_converts_known_forms() {
    assert_eq!(csharpize_statement("   "), "");
    assert_eq!(csharpize_statement("// note"), "// note");
    assert_eq!(csharpize_statement("let x = 1;"), "var x = 1;");
    assert_eq!(csharpize_statement("if t0 {"), "if (t0) {");
    assert_eq!(csharpize_statement("while t1 {"), "while (t1) {");
    assert_eq!(
        csharpize_statement("for (let i = 0; i < 3; i++) {"),
        "for (var i = 0; i < 3; i++) {"
    );
}

#[test]
fn csharp_includes_offsetless_manifest_methods_as_stubs() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "Stubby",
                "abi": {
                    "methods": [
                        { "name": "main", "parameters": [], "returntype": "Void", "offset": 0 },
                        { "name": "helper", "parameters": [], "returntype": "Void" }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*"
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("public static void helper()"),
        "offsetless method should appear in C# skeleton"
    );
    assert!(
        csharp.contains("NotImplementedException"),
        "offsetless method should be emitted as a stub"
    );
}

#[test]
fn csharp_includes_manifest_events() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "Events",
                "abi": {
                    "methods": [
                        { "name": "main", "parameters": [], "returntype": "Void", "offset": 0 }
                    ],
                    "events": [
                        {
                            "name": "transfer-event",
                            "parameters": [
                                { "name": "from", "type": "Hash160" },
                                { "name": "to", "type": "Hash160" },
                                { "name": "amount", "type": "Integer" }
                            ]
                        }
                    ]
                },
                "permissions": [],
                "trusts": "*"
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(csharp.contains("[DisplayName(\"transfer-event\")]"));
    assert!(
        csharp.contains("public static event Action<UInt160, UInt160, BigInteger> transfer_event;")
    );
}

#[test]
fn csharp_escapes_reserved_keywords() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "class",
                "abi": {
                    "methods": [
                        {
                            "name": "class",
                            "parameters": [{ "name": "namespace", "type": "Integer" }],
                            "returntype": "Void",
                            "offset": 0
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*"
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(csharp.contains("public class @class : SmartContract"));
    assert!(csharp.contains("public static void @class(BigInteger @namespace)"));
}

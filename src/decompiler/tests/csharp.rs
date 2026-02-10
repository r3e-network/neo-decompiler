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
    assert_eq!(
        csharpize_statement("leave label_0x0010;"),
        "goto label_0x0010;"
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

#[test]
fn csharp_uses_label_style_for_transfer_placeholders() {
    // Script: ENDTRY +0 (to ENDTRY_L), ENDTRY_L +0 (to RET), RET
    let nef_bytes = build_nef(&[0x3D, 0x00, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x40]);

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");

    assert!(
        csharp.contains("goto label_0x0002;"),
        "C# should normalize leave-transfers to goto label style: {csharp}"
    );
    assert!(
        csharp.contains("goto label_0x0007;"),
        "C# should normalize long leave-transfers to goto label style: {csharp}"
    );
    assert!(
        csharp.contains("label_0x0002:"),
        "C# should emit label declaration for transfer targets: {csharp}"
    );
    assert!(
        csharp.contains("label_0x0007:"),
        "C# should emit label declaration for long transfer targets: {csharp}"
    );
    assert!(
        !csharp.contains("leave label_"),
        "C# should not emit non-C# leave statements: {csharp}"
    );
}

#[test]
fn csharp_mismatch_offset_emits_script_entry_and_manifest_method() {
    // Script: PUSH1; RET; PUSH2; RET
    let nef_bytes = build_nef(&[0x11, 0x40, 0x12, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "OffsetMismatch",
                "abi": {
                    "methods": [
                        {
                            "name": "helper",
                            "parameters": [],
                            "returntype": "Integer",
                            "offset": 2
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
    assert!(
        csharp.contains("public static void ScriptEntry()"),
        "C# output should keep a synthetic script-entry method when ABI offsets do not include bytecode entry"
    );
    assert!(
        csharp.contains("public static BigInteger helper()"),
        "C# output should still emit the manifest method"
    );

    let before_helper = csharp
        .split("public static BigInteger helper")
        .next()
        .expect("entry section present");
    assert!(
        before_helper.contains("// 0000: PUSH1"),
        "script-entry body should contain bytecode from script start"
    );
    assert!(
        !before_helper.contains("// 0002: PUSH2"),
        "script-entry body should stop before helper method offset"
    );
}

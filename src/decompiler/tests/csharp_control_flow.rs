use super::super::*;
use super::*;

#[test]
fn csharp_translates_loop_to_while_true() {
    // Script: INITSLOT; PUSH0; STLOC0; (loop top:) LDLOC0; PUSH3; LT;
    // JMPIFNOT to JMP; NOP; LDLOC0; PUSH1; ADD; STLOC0; JMP back-to-PUSH0; RET.
    // The JMP here targets the `STLOC0` initialization (an infinite reset
    // loop), so the high-level post-pass collapses the `label: ... goto label;`
    // pattern into `loop { ... }`. The C# emitter must rewrite that into a
    // valid C# `while (true)`.
    let script = [
        0x57, 0x01, 0x00, 0x10, 0x70, 0x68, 0x13, 0xB5, 0x26, 0x07, 0x21, 0x68, 0x11, 0x9E, 0x70,
        0x22, 0xF4, 0x40,
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("while (true) {"),
        "C# output should translate `loop {{` to `while (true) {{`: {csharp}"
    );
    assert!(
        !csharp.contains("loop {"),
        "C# output should not retain the high-level `loop` keyword: {csharp}"
    );
}

#[test]
fn csharp_translates_switch_to_idiomatic_c_sharp() {
    // Script: INITSLOT; STLOC0(1); equality chain on loc0 with cases 0,1,default
    let script = [
        0x57, 0x01, 0x00, 0x11, 0x70, 0x68, 0x10, 0x97, 0x26, 0x06, 0x1A, 0x70, 0x22, 0x0D, 0x68,
        0x11, 0x97, 0x26, 0x06, 0x1B, 0x70, 0x22, 0x04, 0x1C, 0x70, 0x68, 0x40,
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("switch (loc0) {"),
        "switch scrutinee should be parenthesised: {csharp}"
    );
    assert!(
        csharp.contains("case var __switchValue0 when ")
            && csharp.contains("new object[] { __switchValue0, 0 }): {"),
        "first case should use guarded VM equality: {csharp}"
    );
    assert!(
        csharp.contains("case var __switchValue1 when ")
            && csharp.contains("new object[] { __switchValue1, 1 }): {"),
        "second case should use guarded VM equality: {csharp}"
    );
    assert!(
        csharp.contains("default: {"),
        "default label should use `default: {{`: {csharp}"
    );
    // Each case body must end in a control-transfer statement; with the
    // simple PUSH/STLOC bodies here the emitter inserts `break;` before the
    // matching close brace so the switch compiles under C#.
    let break_count = csharp.matches("break;").count();
    assert!(
        break_count >= 3,
        "each case (including default) should end with `break;` (found {break_count}): {csharp}"
    );
    assert!(
        !csharp.contains("case 0 {"),
        "C# output should not retain the high-level `case X {{` form: {csharp}"
    );
    assert!(
        !csharp.contains("default {"),
        "C# output should not retain the high-level `default {{` form: {csharp}"
    );
}

#[test]
fn csharp_else_if_chain_uses_parenthesised_conditions() {
    // Same script as switch test — high-level emitter may or may not promote
    // it to a switch depending on heuristics; either way, any surviving
    // `else if` chain must be parenthesised in C#.
    let script = [
        0x57, 0x01, 0x00, 0x11, 0x70, 0x68, 0x10, 0x97, 0x26, 0x06, 0x1A, 0x70, 0x22, 0x0D, 0x68,
        0x11, 0x97, 0x26, 0x06, 0x1B, 0x70, 0x22, 0x04, 0x1C, 0x70, 0x68, 0x40,
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    // No bare `else if X {` (without parens) should survive.
    for line in csharp.lines() {
        let trimmed = line.trim();
        assert!(
            !trimmed.starts_with("else if ") || trimmed.starts_with("else if ("),
            "C# else-if must use parenthesised condition: {trimmed}"
        );
        assert!(
            !trimmed.starts_with("} else if ") || trimmed.starts_with("} else if ("),
            "C# `}} else if` must use parenthesised condition: {trimmed}"
        );
    }
}

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
fn csharp_view_escapes_all_manifest_attribute_controls() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "Demo",
                "abi": {"methods": [], "events": []},
                "extra": {"Note": "line\u0000\u0007\u0008\u000C\n\r\t\u000B\u0001\u2028\u2029"}
            }
            "#,
    )
    .expect("manifest parsed");

    let csharp = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds")
        .csharp
        .expect("csharp output");
    assert!(csharp.contains(r#"[ManifestExtra("Note", "line\0\a\b\f\n\r\t\v\u0001\u2028\u2029")]"#));
}

#[test]
fn high_level_view_renders_manifest_groups_block() {
    // `groups` (signed pubkey memberships authorising contract
    // updates) was dropped from the high-level summary. The C#
    // emitter has no idiomatic place to surface this metadata since
    // it's set at deployment time, not declared in source — but the
    // high-level summary is meant to be a complete inspection view of
    // the manifest, so it should show what the manifest contains.
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "Demo",
                "groups": [
                    {"pubkey": "02f49ce0c33aabbccdd", "signature": "BAt..."},
                    {"pubkey": "02b00b1eaaaabbbbcccc", "signature": "BAd..."}
                ],
                "abi": {"methods": [], "events": []},
                "permissions": [],
                "trusts": "*",
                "extra": {}
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("groups {"),
        "high-level should open a groups block:\n{high_level}"
    );
    assert!(high_level.contains("pubkey=02f49ce0c33aabbccdd"));
    assert!(high_level.contains("pubkey=02b00b1eaaaabbbbcccc"));
    // Signature is intentionally elided — opaque base64, no human value.
    assert!(!high_level.contains("BAt..."));
    assert!(!high_level.contains("signature="));

    // C# header should mirror the high-level rendering with a
    // `// groups:` comment block (parity with the existing
    // `// permissions:` and `// trusts:` blocks). The `groups` field
    // has no source-level attribute in Neo SmartContract Framework
    // (set at deployment, not declared in code), so a comment is the
    // right surface for completeness.
    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("// groups:"),
        "C# header should open a groups comment block:\n{csharp}"
    );
    assert!(csharp.contains("//   pubkey=02f49ce0c33aabbccdd"));
    assert!(csharp.contains("//   pubkey=02b00b1eaaaabbbbcccc"));
    // Same elision policy as high-level: signature is opaque.
    assert!(!csharp.contains("BAt..."));
    assert!(!csharp.contains("signature="));
}

#[test]
fn csharp_view_renders_non_string_scalar_extra_metadata() {
    // Manifests in the wild occasionally carry numeric or boolean
    // entries in `extra` (e.g. `"Version": 1`, `"Verified": true`).
    // The renderer used to gate on `value.as_str()` and silently drop
    // anything else, hiding real metadata. Now both renderers
    // stringify scalars via `render_extra_scalar`, so users see the
    // value verbatim.
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "Demo",
                "abi": {"methods": [], "events": []},
                "permissions": [],
                "trusts": "*",
                "extra": {
                    "Author": "Anon",
                    "Version": 2,
                    "Verified": true,
                    "Notes": null
                }
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(csharp.contains("[ManifestExtra(\"Author\", \"Anon\")]"));
    assert!(csharp.contains("[ManifestExtra(\"Version\", \"2\")]"));
    assert!(csharp.contains("[ManifestExtra(\"Verified\", \"true\")]"));
    // null has no canonical short form — entry is dropped, not rendered as "null".
    assert!(!csharp.contains("ManifestExtra(\"Notes\""));

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(high_level.contains("// Author: Anon"));
    assert!(high_level.contains("// Version: 2"));
    assert!(high_level.contains("// Verified: true"));
    assert!(!high_level.contains("// Notes:"));
}

use super::super::*;
use super::*;

#[test]
fn csharp_trims_initslot_boundaries() {
    let Some(nef_bytes) = try_load_testing_nef("Contract_Delegate.nef") else {
        eprintln!("Skipping: Contract_Delegate.nef not found in devpack artifacts");
        return;
    };
    let Some(manifest) = try_load_testing_manifest("Contract_Delegate.manifest.json") else {
        eprintln!("Skipping: Contract_Delegate.manifest.json not found");
        return;
    };

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    let sum_block = csharp
        .split("public static BigInteger sumFunc")
        .nth(1)
        .and_then(|rest| rest.split("private static dynamic sub_0x000C").next())
        .expect("sumFunc block present");
    assert!(
        sum_block.contains("// 0000: INITSLOT"),
        "sumFunc should still show its entry INITSLOT"
    );
    assert!(
        !sum_block.contains("// 000C: INITSLOT"),
        "sumFunc body should stop before the inferred helper block"
    );
    assert!(
        !sum_block.contains("return t23;"),
        "duplicate return from appended block should not appear in sumFunc"
    );
    assert!(
        csharp.contains("private static dynamic sub_0x000C"),
        "inferred helper should now be emitted separately"
    );
}

#[test]
fn csharp_multi_entry_typed_trusts_render_as_block() {
    // Manifest with structured `trusts: {hashes:[...], groups:[...]}`
    // produces a typed list with 4 entries — too many for a single
    // line, so the C# header should break it into a `// trusts:`
    // block parallel to `// permissions:`.
    let nef_bytes = build_nef(&[0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "MultiTrust",
                "abi": {
                    "methods": [
                        {
                            "name": "main",
                            "parameters": [],
                            "returntype": "Void",
                            "offset": 0
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": {
                    "hashes": ["0xabc", "0xdef"],
                    "groups": ["02foo", "02bar"]
                }
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("// trusts:"),
        "multi-entry typed trusts should render as a block: {csharp}"
    );
    assert!(
        csharp.contains("//   hash:0xabc"),
        "hash entries should be indented under the trusts block: {csharp}"
    );
    assert!(
        csharp.contains("//   hash:0xdef"),
        "hash entries should be indented under the trusts block: {csharp}"
    );
    assert!(
        csharp.contains("//   group:02foo"),
        "group entries should be indented under the trusts block: {csharp}"
    );
    assert!(
        csharp.contains("//   group:02bar"),
        "group entries should be indented under the trusts block: {csharp}"
    );
    assert!(
        !csharp.contains("// trusts = [hash:0xabc, hash:0xdef, group:02foo, group:02bar]"),
        "multi-entry trusts must not stretch onto a single line: {csharp}"
    );
}

#[test]
fn header_surfaces_nef_compiler_and_source_fields() {
    // The NEF header carries `compiler` (always set in practice) and
    // `source` (often a repo URL or commit hash, sometimes empty).
    // Both fields are visible via `info` but were dropped from the
    // decompiled headers, leaving readers to run a separate command
    // to learn what produced the bytecode. Surface them as comment
    // lines under the script hash. Empty fields are silently
    // skipped (the test harness's `build_nef` writes `compiler =
    // "test"` and an empty source, so we exercise the present /
    // absent paths together).
    let nef_bytes = sample_nef();
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("    // compiler: test"),
        "high-level should surface the NEF compiler field:\n{high_level}"
    );
    assert!(
        !high_level.contains("    // source:"),
        "empty source should not emit a placeholder line:\n{high_level}"
    );

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("        // compiler: test"),
        "C# header should surface the NEF compiler field at the C# indent:\n{csharp}"
    );
    assert!(
        !csharp.contains("// source:"),
        "empty source should not emit a placeholder line in C#:\n{csharp}"
    );
}

#[test]
fn csharp_header_surfaces_inferred_patterns_and_language() {
    let mut nef_bytes = sample_nef();
    let compiler = b"Neo.Compiler.CSharp 3";
    nef_bytes[4..4 + compiler.len()].copy_from_slice(compiler);
    let checksum_offset = nef_bytes.len() - std::mem::size_of::<u32>();
    let checksum = NefParser::calculate_checksum(&nef_bytes[..checksum_offset]);
    nef_bytes[checksum_offset..].copy_from_slice(&checksum.to_le_bytes());
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "Token",
            "supportedstandards": ["NEP-17"],
            "abi": {
                "methods": [
                    {"name":"symbol","parameters":[],"returntype":"String","offset":0},
                    {"name":"decimals","parameters":[],"returntype":"Integer","offset":1},
                    {"name":"totalSupply","parameters":[],"returntype":"Integer","offset":2},
                    {"name":"balanceOf","parameters":[],"returntype":"Integer","offset":3},
                    {"name":"transfer","parameters":[],"returntype":"Boolean","offset":4}
                ],
                "events": []
            }
        }"#,
    )
    .expect("manifest parsed");
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");
    let csharp = decompilation.csharp.as_deref().expect("csharp output");

    assert!(csharp.contains("        // inferred standards: NEP-17"));
    assert!(csharp.contains("        // inferred patterns: NEP-17"));
    assert!(csharp.contains("        // inferred language: C#"));
    assert!(csharp.contains("        // pattern confidence: high"));
}

#[test]
fn csharp_header_renders_method_tokens_block() {
    // The high-level renderer surfaces `// method tokens declared in
    // NEF` so a reader can cross-reference each CALLT call against
    // its native contract / call flags. The C# header silently
    // dropped the table, leaving readers to scrape the NEF
    // separately. Render it as a comment block (parity with the
    // existing `// permissions:` / `// groups:` blocks) — Neo
    // SmartContract Framework has no source-level construct for
    // method tokens, so a comment is the correct surface.
    //
    // Hash chosen to match the StdLib native contract so the
    // renderer adds the friendly contract label.
    let stdlib_hash: [u8; 20] = [
        0xC0, 0xEF, 0x39, 0xCE, 0xE0, 0xE4, 0xE9, 0x25, 0xC6, 0xC2, 0xA0, 0x6A, 0x79, 0xE1, 0x44,
        0x0D, 0xD8, 0x6F, 0xCE, 0xAC,
    ];
    let nef_bytes = build_nef_with_single_token(&[0x40], stdlib_hash, "Serialize", 1, true, 0x0F);

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("// method tokens declared in NEF:"),
        "C# header should open the method tokens comment block:\n{csharp}"
    );
    assert!(
        csharp.contains(
            "//   Serialize (StdLib::Serialize) hash=C0EF39CEE0E4E925C6C2A06A79E1440DD86FCEAC params=1 returns=true flags=0x0F"
        ),
        "method token line should match high-level layout (with native contract label):\n{csharp}"
    );
    assert!(
        csharp.contains("(ReadStates|WriteStates|AllowCall|AllowNotify)"),
        "call flags should be described:\n{csharp}"
    );
}

#[test]
fn csharp_header_omits_method_tokens_block_when_none() {
    // Empty token table => no header line at all (don't leave a
    // `// method tokens declared in NEF:` orphan).
    let nef_bytes = sample_nef();
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        !csharp.contains("method tokens declared in NEF"),
        "no token block expected when NEF has no method tokens:\n{csharp}"
    );
}

#[test]
fn csharp_single_entry_typed_trusts_stay_on_one_line() {
    // Single-entry typed lists are short — keep them on one line so
    // the header doesn't grow unnecessarily for the common case.
    let nef_bytes = build_nef(&[0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "SingleTrust",
                "abi": {
                    "methods": [
                        {
                            "name": "main",
                            "parameters": [],
                            "returntype": "Void",
                            "offset": 0
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": { "groups": ["02abcdef"] }
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("// trusts = [group:02abcdef]"),
        "single-entry trusts should stay compact on one line: {csharp}"
    );
    assert!(
        !csharp.contains("// trusts:"),
        "single-entry trusts should not break into a block: {csharp}"
    );
}

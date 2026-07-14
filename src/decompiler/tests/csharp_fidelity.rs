use super::super::*;
use super::*;

#[test]
fn csharp_invalid_any_convert_and_istype_use_whole_method_fallback() {
    for opcode in [
        crate::instruction::OpCode::Convert,
        crate::instruction::OpCode::Istype,
    ] {
        let script = [
            crate::instruction::OpCode::Push1.byte(),
            opcode.byte(),
            0x00,
            crate::instruction::OpCode::Ret.byte(),
        ];
        let rendered = render_csharp_with_coverage(&build_nef(&script), None, true, false, true);
        let coverage = rendered
            .coverage
            .method(0, "ScriptEntry")
            .expect("entry coverage");

        assert_eq!(
            coverage.backend,
            crate::decompiler::csharp::BodyBackend::Structured,
            "{opcode:?}: {coverage:#?}"
        );
        assert_eq!(
            coverage.fidelity.status,
            crate::decompiler::cfg::method_body::Fidelity::Incomplete
        );
    }
}

#[test]
fn csharp_unknown_source_and_unresolved_call_use_whole_method_fallback() {
    let unknown_source =
        render_csharp_with_coverage(&build_nef(&[0x68, 0x40]), None, true, false, true);
    let unknown_source_coverage = unknown_source
        .coverage
        .method(0, "ScriptEntry")
        .expect("unknown-source coverage");
    assert_eq!(
        unknown_source_coverage.backend,
        crate::decompiler::csharp::BodyBackend::Structured,
        "{unknown_source_coverage:#?}\n{}",
        unknown_source.source
    );
    assert_eq!(
        unknown_source_coverage.fidelity.status,
        crate::decompiler::cfg::method_body::Fidelity::Incomplete
    );
    let unknown_source_issue = unknown_source_coverage
        .primary_issue
        .as_ref()
        .expect("unknown-source primary issue");
    assert_eq!(unknown_source_issue.offset, 0);
    assert_eq!(
        unknown_source_issue.opcode,
        crate::instruction::OpCode::Ldloc0
    );
    assert_eq!(
        unknown_source_issue.kind,
        crate::decompiler::cfg::method_body::LoweringIssueKind::LostStackValue
    );
    assert!(unknown_source.source.contains("Runtime.LoadScript"));
    assert!(!unknown_source.source.contains("???"));

    let unresolved_call =
        render_csharp_with_coverage(&build_nef(&[0x34, 0x7F, 0x40]), None, true, false, true);
    let unresolved_call_coverage = unresolved_call
        .coverage
        .method(0, "ScriptEntry")
        .expect("unresolved-call coverage");
    assert_eq!(
        unresolved_call_coverage.backend,
        crate::decompiler::csharp::BodyBackend::Structured
    );
    assert_eq!(
        unresolved_call_coverage.fidelity.status,
        crate::decompiler::cfg::method_body::Fidelity::Incomplete
    );
    let unresolved_call_issue = unresolved_call_coverage
        .primary_issue
        .as_ref()
        .expect("unresolved-call primary issue");
    assert_eq!(unresolved_call_issue.offset, 0);
    assert_eq!(
        unresolved_call_issue.opcode,
        crate::instruction::OpCode::Call
    );
    assert_eq!(
        unresolved_call_issue.kind,
        crate::decompiler::cfg::method_body::LoweringIssueKind::UnresolvedCall
    );
    assert!(unresolved_call
        .source
        .contains("__NeoDecompilerUnresolvedCall"));
}

#[test]
fn csharp_typed_map_temporary_keeps_its_receiver_type() {
    let script = [
        crate::instruction::OpCode::Newmap.byte(),
        crate::instruction::OpCode::Push1.byte(),
        crate::instruction::OpCode::Haskey.byte(),
        crate::instruction::OpCode::Ret.byte(),
    ];
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "TypedMapTemporary",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Boolean", "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered =
        render_csharp_with_coverage(&build_nef(&script), Some(manifest), false, false, true);

    assert_eq!(
        rendered
            .coverage
            .method(0, "main")
            .expect("main coverage")
            .backend,
        crate::decompiler::csharp::BodyBackend::Structured
    );
    assert!(
        rendered
            .source
            .contains("Map<object, object> t_0 = new Map<object, object>();"),
        "{}",
        rendered.source
    );
    assert!(
        rendered.source.contains("return t_0.HasKey(1);"),
        "{}",
        rendered.source
    );
}

#[test]
fn csharp_fallback_primary_issue_is_the_first_incomplete_issue() {
    let script = [
        crate::instruction::OpCode::Syscall.byte(),
        0xB2,
        0x79,
        0xFC,
        0xF6,
        crate::instruction::OpCode::Pack.byte(),
        crate::instruction::OpCode::Ret.byte(),
    ];

    let rendered = render_csharp_with_coverage(&build_nef(&script), None, true, false, true);
    let coverage = rendered
        .coverage
        .method(0, "ScriptEntry")
        .expect("entry coverage");
    let primary = coverage.primary_issue.as_ref().expect("primary issue");

    assert_eq!(
        coverage.backend,
        crate::decompiler::csharp::BodyBackend::Structured
    );
    assert_eq!(primary.offset, 5);
    assert_eq!(primary.opcode, crate::instruction::OpCode::Pack);
    assert_eq!(
        primary.kind,
        crate::decompiler::cfg::method_body::LoweringIssueKind::MissingProvenance
    );
    assert_eq!(
        primary.fidelity,
        crate::decompiler::cfg::method_body::Fidelity::Incomplete
    );
    assert!(rendered.source.contains("Runtime.LoadScript"));
    assert!(!rendered
        .warnings
        .iter()
        .any(|warning| warning.contains("used legacy body")));
}

#[test]
fn csharp_coverage_retains_same_name_overloads_at_one_offset() {
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "CoverageOverloads",
            "abi": { "methods": [
                {
                    "name": "read", "parameters": [{"name": "key", "type": "Integer"}],
                    "returntype": "Void", "offset": 0
                },
                {
                    "name": "read", "parameters": [{"name": "key", "type": "String"}],
                    "returntype": "Void", "offset": 0
                }
            ] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered = render_csharp_with_coverage(
        &build_nef(&[crate::instruction::OpCode::Ret.byte()]),
        Some(manifest),
        true,
        false,
        true,
    );
    let coverage_count = rendered
        .coverage
        .methods
        .values()
        .map(std::collections::BTreeMap::len)
        .sum::<usize>();

    assert_eq!(
        rendered.source.matches("public static void read(").count(),
        2
    );
    assert_eq!(coverage_count, 2, "{:#?}", rendered.coverage);
}

#[test]
fn csharp_synthetic_script_entry_exposes_initslot_args_and_preserves_return() {
    // Bytecode: INITSLOT 0,1; LDARG0; ISNULL; NOT; RET — declares one
    // argument and returns its non-null-ness. Without a manifest, the
    // C# emitter must (a) surface the arg as a parameter so the body's
    // `arg0` reference resolves, and (b) preserve the lifted return
    // value (the previous hardcoded `void` signature silently dropped
    // it).
    let nef_bytes = build_nef(&[0x57, 0x00, 0x01, 0x78, 0xD8, 0xAA, 0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("public static object ScriptEntry(object arg0)"),
        "synthetic ScriptEntry should declare INITSLOT-counted args: {csharp}"
    );
    // Verbose-mode (lib API default) keeps the temp; clean-mode would
    // inline. Either form preserves the return value, which is the
    // bug we care about — the previous `void` signature dropped it.
    assert!(
        csharp.contains("return !t0;")
            || csharp.contains("return !t_0;")
            || csharp.contains("return t_1;")
            || csharp.contains("return !(arg0 is null);"),
        "lifted return value should be preserved (not dropped via void signature): {csharp}"
    );
    // Also verify the body actually references arg0 (not just the
    // signature) so the param isn't unused boilerplate.
    assert!(
        csharp.contains("(arg0 is null)")
            || csharp.contains("arg0 is null")
            || csharp.contains("is_null(arg0)"),
        "body should reference arg0 from the new parameter: {csharp}"
    );
}

#[test]
fn csharp_omits_trailing_return_in_void_methods() {
    // Smallest script: a single RET. Without a manifest the synthetic
    // ScriptEntry now defaults to `object` return (so any pushed value
    // is preserved instead of dropped) — for a bare RET that produces
    // `return default;` in the body. The historical behavior (`void`
    // signature with `return;` stripped) was buggy for non-void scripts
    // (it silently discarded the lifted return value).
    let nef_bytes = build_nef(&[0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("public static object ScriptEntry()"),
        "synthetic ScriptEntry should default to object return when no manifest is provided: {csharp}"
    );
    // Bare RET on empty stack lifts to `return;`. With a non-void
    // signature the C# render rewrites that to `return default;` to
    // satisfy the type system.
    assert!(
        csharp.contains("return default;"),
        "synthetic ScriptEntry with bare RET should yield `return default;`: {csharp}"
    );
}

#[test]
fn csharp_private_void_call_preserves_ambient_return_value() {
    let nef_bytes = build_nef(&[
        0x19, 0x11, 0x34, 0x05, 0x40, 0x21, 0x21, 0x57, 0x00, 0x01, 0x78, 0x45, 0x40,
    ]);
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "InferredVoidHelper",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Integer", "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .with_inline_single_use_temps(true)
        .with_trace_comments(false)
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");
    let csharp = decompilation.csharp.as_deref().expect("csharp output");

    assert!(
        csharp.contains("sub_0x0007(1);") && csharp.contains("return 9;"),
        "private void call must preserve the caller's ambient value: {csharp}"
    );
    assert!(
        csharp.contains("private static void sub_0x0007(dynamic arg0)")
            && !csharp.contains("return sub_0x0007(1)"),
        "private helper must have a void C# signature: {csharp}"
    );
}

#[test]
fn csharp_keeps_explicit_return_value_in_non_void_methods() {
    // Script: PUSH1 RET — high-level lifts as `return 1;`, which the C#
    // emitter must preserve since the method is non-void.
    let nef_bytes = build_nef(&[0x11, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "ReturnsInt",
                "abi": {
                    "methods": [
                        {
                            "name": "main",
                            "parameters": [],
                            "returntype": "Integer",
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
    assert!(
        csharp.contains("return 1;"),
        "non-void method should keep its computed return: {csharp}"
    );
}

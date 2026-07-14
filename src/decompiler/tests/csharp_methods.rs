use super::super::*;
use super::*;

#[test]
fn csharp_resolves_internal_calls_to_method_names() {
    // Script layout:
    // 0x0000: CALL +4 (target=0x0004)
    // 0x0002: RET
    // 0x0003: NOP
    // 0x0004: RET
    let script = [0x34, 0x04, 0x40, 0x21, 0x40];
    let nef_bytes = build_nef(&script);

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("sub_0x0004()"),
        "C# output should resolve internal helper names instead of raw call placeholders: {csharp}"
    );
    assert!(
        !csharp.contains("call_0x0004"),
        "C# output should not emit raw call_0x placeholders when a helper name is known: {csharp}"
    );
}

#[test]
fn csharp_internal_call_uses_duplicate_signature_suffix() {
    // main calls the third `transfer` declaration. The first and second are
    // valid C# overloads, while the third duplicates the first signature and
    // therefore needs the same ordinal suffix at both definition and call site.
    let nef_bytes = build_nef(&[
        0x11, 0x34, 0x0D, 0x40, 0x57, 0x00, 0x01, 0x78, 0x40, 0x57, 0x00, 0x01, 0x78, 0x40, 0x57,
        0x00, 0x01, 0x78, 0x40,
    ]);
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "OverloadCall",
            "abi": { "methods": [
                {
                    "name": "main",
                    "parameters": [],
                    "returntype": "Integer",
                    "offset": 0
                },
                {
                    "name": "transfer",
                    "parameters": [{ "name": "value", "type": "Integer" }],
                    "returntype": "Integer",
                    "offset": 4
                },
                {
                    "name": "transfer",
                    "parameters": [{ "name": "enabled", "type": "Boolean" }],
                    "returntype": "Integer",
                    "offset": 9
                },
                {
                    "name": "transfer",
                    "parameters": [{ "name": "value", "type": "Integer" }],
                    "returntype": "Integer",
                    "offset": 14
                }
            ] }
        }"#,
    )
    .expect("manifest parsed");

    let csharp = Decompiler::new()
        .with_inline_single_use_temps(true)
        .with_trace_comments(false)
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds")
        .csharp
        .expect("csharp output");

    assert!(
        csharp.contains("public static BigInteger transfer_2(BigInteger @value)"),
        "duplicate signature must use the final ordinal suffix: {csharp}"
    );
    assert!(
        csharp.contains("return transfer_2(1);"),
        "internal call must use the declaration's final emitted name: {csharp}"
    );
}

#[test]
fn csharp_manifest_void_internal_call_is_a_statement() {
    // 0x0000: CALL +3 (helper at 0x0003)
    // 0x0002: RET
    // 0x0003: RET
    let nef_bytes = build_nef(&[0x34, 0x03, 0x40, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "VoidCall",
                "abi": {
                    "methods": [
                        { "name": "main", "parameters": [], "returntype": "Void", "offset": 0 },
                        { "name": "helper", "parameters": [], "returntype": "Void", "offset": 3 }
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
        csharp.contains("            helper();"),
        "void internal CALL should render as a statement: {csharp}"
    );
    assert!(
        !csharp.contains(" = helper();"),
        "void internal CALL must not be assigned to a temp: {csharp}"
    );
}

#[test]
fn csharp_manifest_void_resolved_calla_is_a_statement() {
    // 0x0000: PUSHA +10 (helper at 0x000A)
    // 0x0005: CALLA
    // 0x0006: RET
    // 0x0007..0x0009: NOP padding
    // 0x000A: INITSLOT 0,0; RET
    let nef_bytes = build_nef(&[
        0x0A, 0x0A, 0x00, 0x00, 0x00, 0x36, 0x40, 0x21, 0x21, 0x21, 0x57, 0x00, 0x00, 0x40,
    ]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "VoidCallA",
                "abi": {
                    "methods": [
                        { "name": "main", "parameters": [], "returntype": "Void", "offset": 0 },
                        { "name": "helper", "parameters": [], "returntype": "Void", "offset": 10 }
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
        csharp.contains("            helper();"),
        "manifest-known void CALLA target should render as a statement: {csharp}"
    );
    assert!(
        !csharp.contains(" = helper();"),
        "manifest-known void CALLA target must not be assigned to a temp: {csharp}"
    );
}

#[test]
fn csharp_offsetless_manifest_void_internal_call_is_a_statement() {
    // All ABI offsets are absent, so the first manifest method names the
    // script entry at 0x0000. The recursive CALL must inherit its void return.
    let nef_bytes = build_nef(&[0x34, 0x00, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "OffsetlessVoidCall",
                "abi": {
                    "methods": [
                        { "name": "main", "parameters": [], "returntype": "Void" }
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
        csharp.contains("            main();"),
        "offsetless manifest entry CALL should render as a void statement: {csharp}"
    );
    assert!(
        !csharp.contains(" = main();"),
        "offsetless void entry CALL must not be assigned to a temp: {csharp}"
    );
}

#[test]
fn csharp_offsetless_manifest_void_resolved_calla_is_a_statement() {
    // PUSHA 0 resolves CALLA back to the offsetless manifest entry.
    let nef_bytes = build_nef(&[0x0A, 0x00, 0x00, 0x00, 0x00, 0x36, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "OffsetlessVoidCallA",
                "abi": {
                    "methods": [
                        { "name": "main", "parameters": [], "returntype": "Void" }
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
        csharp.contains("            main();"),
        "offsetless manifest entry CALLA should render as a void statement: {csharp}"
    );
    assert!(
        !csharp.contains(" = main();"),
        "offsetless void entry CALLA must not be assigned to a temp: {csharp}"
    );
}

#[test]
fn csharp_manifest_void_tail_call_does_not_return_ambient_stack_value() {
    // main: PUSH1; JMP +2 -> helper@3. A void tail call produces no value, so
    // the pre-existing PUSH1 must not be consumed as the method's return.
    let nef_bytes = build_nef(&[0x11, 0x22, 0x02, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "VoidTailCall",
                "abi": {
                    "methods": [
                        { "name": "main", "parameters": [], "returntype": "Void", "offset": 0 },
                        { "name": "helper", "parameters": [], "returntype": "Void", "offset": 3 }
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
        csharp.contains("            helper();"),
        "void tail call should remain visible as a statement: {csharp}"
    );
    assert!(
        !csharp.contains("return t0;") && !csharp.contains("return 1;"),
        "void tail call must not return an unrelated ambient stack value: {csharp}"
    );
}

#[test]
fn csharp_manifest_void_internal_call_underflow_keeps_call_visible() {
    // helper@0 declares one argument. caller@4 invokes it with an empty
    // evaluation stack; the decompiler must retain the call shape and mark the
    // missing argument instead of silently dropping the CALL.
    let nef_bytes = build_nef(&[0x57, 0x00, 0x01, 0x40, 0x34, 0xFC, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "VoidCallUnderflow",
                "abi": {
                    "methods": [
                        {
                            "name": "helper",
                            "parameters": [{ "name": "value", "type": "Integer" }],
                            "returntype": "Void",
                            "offset": 0
                        },
                        {
                            "name": "caller",
                            "parameters": [],
                            "returntype": "Void",
                            "offset": 4
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
        csharp.contains("helper((dynamic)null);"),
        "argument underflow must remain a typed structured call: {csharp}"
    );
}

#[test]
fn csharp_manifest_value_tail_call_underflow_still_returns_call() {
    // caller@4 tail-jumps to helper@0 without supplying helper's declared
    // argument. Underflow must not be mistaken for a void return contract.
    let nef_bytes = build_nef(&[0x57, 0x00, 0x01, 0x40, 0x22, 0xFC]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "ValueTailCallUnderflow",
                "abi": {
                    "methods": [
                        {
                            "name": "helper",
                            "parameters": [{ "name": "value", "type": "Integer" }],
                            "returntype": "Integer",
                            "offset": 0
                        },
                        {
                            "name": "caller",
                            "parameters": [],
                            "returntype": "Integer",
                            "offset": 4
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
        csharp.contains("return helper((dynamic)null);"),
        "argument underflow must preserve value-producing tail-call behavior: {csharp}"
    );
}

#[test]
fn csharp_manifest_value_internal_call_still_produces_a_value() {
    // 0x0000: CALL +3 (helper at 0x0003)
    // 0x0002: RET
    // 0x0003: PUSH1; RET
    let nef_bytes = build_nef(&[0x34, 0x03, 0x40, 0x11, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "ValueCall",
                "abi": {
                    "methods": [
                        { "name": "main", "parameters": [], "returntype": "Integer", "offset": 0 },
                        { "name": "helper", "parameters": [], "returntype": "Integer", "offset": 3 }
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
        csharp.contains("return helper();"),
        "the internal CALL result should remain on the lifted stack: {csharp}"
    );
}

#[test]
fn csharp_unknown_resolved_calla_still_produces_a_value() {
    // No manifest describes the target, so return behavior is unknown and must
    // conservatively remain value-producing.
    let nef_bytes = build_nef(&[
        0x0A, 0x0A, 0x00, 0x00, 0x00, 0x36, 0x40, 0x21, 0x21, 0x21, 0x57, 0x00, 0x00, 0x11, 0x40,
    ]);

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("return sub_0x000A();"),
        "the unknown CALLA result should remain on the lifted stack: {csharp}"
    );
}

#[test]
fn csharp_emits_inferred_helper_methods() {
    // Script layout:
    // 0x0000: CALL +4 (target=0x0004)
    // 0x0002: RET
    // 0x0003: NOP
    // 0x0004: RET
    let script = [0x34, 0x04, 0x40, 0x21, 0x40];
    let nef_bytes = build_nef(&script);

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("private static dynamic sub_0x0004()")
            || csharp.contains("private static void sub_0x0004()")
            || csharp.contains("private static BigInteger sub_0x0004()")
            || csharp.contains("private static object sub_0x0004()"),
        "C# output should emit inferred helper method definitions for resolved internal calls: {csharp}"
    );
    assert!(
        !csharp.contains("sub_0x0003"),
        "C# output should not emit nop-only inferred helper methods: {csharp}"
    );
}

#[test]
fn csharp_inferred_nonvoid_helpers_do_not_emit_bare_return() {
    // Script layout:
    // 0x0000: CALL +4 (target=0x0004)
    // 0x0002: RET
    // 0x0003: NOP
    // 0x0004: RET
    let script = [0x34, 0x04, 0x40, 0x21, 0x40];
    let nef_bytes = build_nef(&script);

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        !csharp.contains(
            "private static dynamic sub_0x0004()
        {
            // 0004: RET
            return;"
        ),
        "non-void inferred helper bodies should not emit bare return statements: {csharp}"
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
fn csharp_disambiguates_events_against_contract_members() {
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
                        { "name": "main", "parameters": [] }
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
    assert!(csharp.contains("[DisplayName(\"main\")]"));
    assert!(csharp.contains("public static event Action main_1;"));
    assert!(csharp.contains("public static void main()"));
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
                            "name": "record",
                            "parameters": [{ "name": "await", "type": "Integer" }],
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
    assert!(csharp.contains("public static void @record(BigInteger @await)"));
}

#[test]
fn csharp_uses_label_style_for_transfer_placeholders() {
    // Script: ENDTRY +6 (jumps past intermediate code to the final RET),
    //         PUSH1, PUSH2, ADD, DROP, RET. The intermediate stack ops
    //         keep the `leave` from being a fallthrough so the lift
    //         exercises the label-style transfer path the C# emitter
    //         lowers to `goto label_X;`.
    let nef_bytes = build_nef(&[0x3D, 0x06, 0x11, 0x12, 0x9E, 0x75, 0x40]);

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");

    assert!(
        csharp.contains("goto label_0x0006;"),
        "C# should normalize leave-transfers to goto label style: {csharp}"
    );
    assert!(
        csharp.contains("label_0x0006:"),
        "C# should emit label declaration for transfer targets: {csharp}"
    );
    assert!(
        !csharp.contains("leave label_"),
        "C# should not emit non-C# leave statements: {csharp}"
    );
    assert!(
        !csharp.contains("leave_0x"),
        "C# should not emit legacy function-style transfer placeholders: {csharp}"
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
    // Synthetic ScriptEntry now uses `object` return type — without
    // a matching manifest entry the emitter doesn't know whether
    // the script's RET is meant to discard or carry a value, so
    // `object` preserves what the bytecode pushed.
    assert!(
        csharp.contains("public static object ScriptEntry()"),
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

#[test]
fn csharp_missing_manifest_offset_uses_first_method_as_entry_signature() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "OffsetMissing",
                "abi": {
                    "methods": [
                        {
                            "name": "main",
                            "parameters": [],
                            "returntype": "Integer"
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
        csharp.contains("public static BigInteger main()"),
        "C# output should reuse the first manifest method signature when offsets are missing"
    );
    assert!(
        !csharp.contains("public static void ScriptEntry()"),
        "synthetic ScriptEntry should not be emitted when the manifest omits entry offsets entirely"
    );
    assert!(
        !csharp.contains("NotImplementedException"),
        "the fallback entry method should not also be emitted as an offset-less stub"
    );
}

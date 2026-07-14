use super::*;

fn render_csharp_with_coverage(
    nef_bytes: &[u8],
    manifest: Option<ContractManifest>,
    inline_single_use_temps: bool,
    emit_trace_comments: bool,
    typed_declarations: bool,
) -> crate::decompiler::csharp::CSharpRender {
    let decompilation = Decompiler::new()
        .with_inline_single_use_temps(inline_single_use_temps)
        .with_trace_comments(emit_trace_comments)
        .with_typed_declarations(typed_declarations)
        .decompile_bytes_with_manifest(nef_bytes, manifest, OutputFormat::All)
        .expect("decompile succeeds");
    crate::decompiler::csharp::render_csharp(
        &decompilation.nef,
        &decompilation.instructions,
        decompilation.manifest.as_ref(),
        &decompilation.call_graph,
        &decompilation.method_contracts,
        &decompilation.types,
        &crate::decompiler::output_format::RenderOptions {
            inline_single_use_temps,
            emit_trace_comments,
            typed_declarations,
        },
    )
}

#[test]
fn csharp_multimethod_uses_structured_constant_fold() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let nef = fs::read(root.join("TestingArtifacts/edgecases/multi/MultiMethod.nef"))
        .expect("MultiMethod NEF");
    let manifest =
        fs::read_to_string(root.join("TestingArtifacts/edgecases/multi/MultiMethod.manifest.json"))
            .expect("MultiMethod manifest");
    let manifest = ContractManifest::from_json_str(&manifest).expect("manifest parsed");

    let rendered = render_csharp_with_coverage(&nef, Some(manifest), true, false, true);

    assert!(
        rendered.source.contains("return 2;"),
        "optimized helper must render its folded return: {}",
        rendered.source
    );
    assert_eq!(
        rendered
            .coverage
            .method(6, "helper")
            .expect("helper coverage")
            .backend,
        crate::decompiler::csharp::BodyBackend::Structured
    );
}

#[test]
fn csharp_typed_declarations_preserve_loop_counter_type() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let nef = fs::read(root.join("TestingArtifacts/edgecases/LoopIf.nef")).expect("LoopIf NEF");
    let manifest = fs::read_to_string(root.join("TestingArtifacts/edgecases/LoopIf.manifest.json"))
        .expect("LoopIf manifest");
    let manifest = ContractManifest::from_json_str(&manifest).expect("manifest parsed");

    let untyped = render_csharp_with_coverage(&nef, Some(manifest.clone()), true, false, false);
    assert!(
        untyped.source.contains("dynamic loc0;"),
        "{}",
        untyped.source
    );

    let typed = render_csharp_with_coverage(&nef, Some(manifest), true, false, true);
    assert!(
        typed.source.contains("BigInteger loc0;"),
        "{}",
        typed.source
    );
}

#[test]
fn csharp_loopif_deversions_local_slot() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let nef = fs::read(root.join("TestingArtifacts/edgecases/LoopIf.nef")).expect("LoopIf NEF");
    let manifest = fs::read_to_string(root.join("TestingArtifacts/edgecases/LoopIf.manifest.json"))
        .expect("LoopIf manifest");
    let manifest = ContractManifest::from_json_str(&manifest).expect("manifest parsed");

    let rendered = render_csharp_with_coverage(&nef, Some(manifest), true, false, true);

    assert_eq!(
        rendered.source.matches("BigInteger loc0;").count(),
        1,
        "mutable local must be declared exactly once: {}",
        rendered.source
    );
    assert!(!rendered.source.contains("loc0_"), "{}", rendered.source);
    assert!(
        !rendered.source.contains("t_4"),
        "adjacent copy temporary must be inlined: {}",
        rendered.source
    );
    assert_eq!(
        rendered
            .coverage
            .method(0, "main")
            .expect("main coverage")
            .backend,
        crate::decompiler::csharp::BodyBackend::Structured
    );
}

#[test]
fn csharp_assert_uses_structured_body() {
    let nef = build_nef(&[0x09, 0x39, 0x11, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "AssertFallback",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Integer", "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered = render_csharp_with_coverage(&nef, Some(manifest), true, false, true);

    let coverage = rendered.coverage.method(0, "main").expect("main coverage");
    assert_eq!(
        coverage.backend,
        crate::decompiler::csharp::BodyBackend::Structured
    );
    assert_eq!(
        coverage.fidelity.status,
        crate::decompiler::cfg::method_body::Fidelity::Exact
    );
    assert!(
        rendered
            .source
            .contains("global::Neo.SmartContract.Framework.ExecutionEngine.Assert(false);"),
        "{}",
        rendered.source
    );
    assert!(
        !rendered.source.contains("throw new Exception"),
        "ASSERT must remain an uncatchable VM assertion: {}",
        rendered.source
    );
    assert!(rendered.source.contains("return 1;"), "{}", rendered.source);
    assert!(!rendered
        .warnings
        .iter()
        .any(|warning| warning.contains("ASSERT")));
}

#[test]
fn csharp_separates_clr_exception_transport_from_vm_payload() {
    let nef = build_nef(&[0x3B, 0x06, 0x00, 0x21, 0x3D, 0x05, 0x45, 0x3D, 0x02, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "CatchState",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Void", "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered = render_csharp_with_coverage(&nef, Some(manifest), true, false, true);
    let coverage = rendered.coverage.method(0, "main").expect("main coverage");

    assert_eq!(
        coverage.backend,
        crate::decompiler::csharp::BodyBackend::Structured
    );
    assert_eq!(
        coverage.fidelity.status,
        crate::decompiler::cfg::method_body::Fidelity::Exact
    );
    assert!(
        rendered
            .source
            .contains("private sealed class __NeoDecompilerVmException : Exception"),
        "{}",
        rendered.source
    );
    assert!(
        rendered
            .source
            .contains("catch (Exception __caughtException"),
        "{}",
        rendered.source
    );
    assert!(
        rendered.source.contains("dynamic exception_b"),
        "{}",
        rendered.source
    );
    assert!(
        rendered.source.contains(".Payload : __caughtException"),
        "{}",
        rendered.source
    );
}

#[test]
fn csharp_assert_preserves_non_scalar_vm_truthiness() {
    // NEWARRAY0; ASSERT; RET. Neo VM compound values are truthy even when
    // empty, so integer-only NZ conversion would fault instead of preserving
    // ASSERT's StackItem.GetBoolean() semantics.
    let nef = build_nef(&[0xC2, 0x39, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "ArrayAssert",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Void", "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered = render_csharp_with_coverage(&nef, Some(manifest), true, false, true);

    assert_eq!(
        rendered
            .coverage
            .method(0, "main")
            .expect("main coverage")
            .backend,
        crate::decompiler::csharp::BodyBackend::Structured
    );
    assert!(
        rendered.source.contains(
            "global::Neo.SmartContract.Framework.ExecutionEngine.Assert((bool)(object)(t_0));"
        ),
        "{}",
        rendered.source
    );
    assert!(
        !rendered.source.contains("Runtime.LoadScript"),
        "ASSERT truthiness must not require caller AllowCall permissions: {}",
        rendered.source
    );
}

#[test]
fn csharp_assert_message_preserves_eager_vm_message_validation() {
    // PUSH1; PUSHDATA1 0xFF; ASSERTMSG; RET. Native ASSERTMSG calls
    // GetString() before checking the condition, so invalid UTF-8 faults even
    // when the condition is true. ExecutionEngine.Assert(bool, string) is not
    // equivalent because Neo.Compiler.CSharp lowers it to JMPIF + ABORTMSG.
    let nef = build_nef(&[0x11, 0x0C, 0x01, 0xFF, 0xE1, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "EagerAssertMessage",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Void", "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered = render_csharp_with_coverage(&nef, Some(manifest), true, false, true);

    assert!(
        rendered.source.contains(
            "[global::Neo.SmartContract.Framework.Attributes.OpCode(global::Neo.SmartContract.Framework.OpCode.ASSERTMSG)]"
        ),
        "ASSERTMSG requires a direct opcode helper: {}",
        rendered.source
    );
    assert!(
        rendered.source.contains(
            "private static extern void __NeoDecompilerAssertMessage(bool condition, string message);"
        ),
        "{}",
        rendered.source
    );
    assert!(
        rendered
            .source
            .contains("            global::NeoDecompiler.Generated.EagerAssertMessage.__NeoDecompilerAssertMessage(1 != 0,"),
        "{}",
        rendered.source
    );
    assert!(
        !rendered.source.contains("ExecutionEngine.Assert(1 != 0,"),
        "the framework overload lazily skips message validation: {}",
        rendered.source
    );
}

#[test]
fn csharp_assert_message_helper_call_ignores_parameter_shadowing() {
    // INITSLOT 0,2; PUSH1; LDARG1; ASSERTMSG; RET. Parameters may legally
    // shadow both the helper member and its containing contract type.
    let nef = build_nef(&[0x57, 0x00, 0x02, 0x11, 0x79, 0xE1, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "HelperNameCollision",
            "abi": { "methods": [{
                "name": "main",
                "parameters": [
                    {
                        "name": "__NeoDecompilerAssertMessage",
                        "type": "Any"
                    },
                    {
                        "name": "HelperNameCollision",
                        "type": "ByteArray"
                    }
                ],
                "returntype": "Void",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered = render_csharp_with_coverage(&nef, Some(manifest), true, false, true);

    assert!(
        rendered.source.contains(
            "global::NeoDecompiler.Generated.HelperNameCollision.__NeoDecompilerAssertMessage(1 != 0, (string)(object)(HelperNameCollision));"
        ),
        "helper calls must not bind to same-named parameters: {}",
        rendered.source
    );
}

#[test]
fn csharp_assert_uses_globally_qualified_framework_intrinsic() {
    let nef = build_nef(&[0x11, 0x39, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "ExecutionEngine",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Void", "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered = render_csharp_with_coverage(&nef, Some(manifest), true, false, true);

    assert!(
        rendered
            .source
            .contains("global::Neo.SmartContract.Framework.ExecutionEngine.Assert(1 != 0);"),
        "framework intrinsic must survive a same-named contract: {}",
        rendered.source
    );
}

#[test]
fn csharp_assert_message_uses_globally_qualified_opcode_attribute() {
    let nef = build_nef(&[0x11, 0x12, 0xE1, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "OpCode",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Void", "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered = render_csharp_with_coverage(&nef, Some(manifest), true, false, true);

    assert!(
        rendered.source.contains(
            "[global::Neo.SmartContract.Framework.Attributes.OpCode(global::Neo.SmartContract.Framework.OpCode.ASSERTMSG)]"
        ),
        "opcode attribute must survive a same-named contract: {}",
        rendered.source
    );
    assert!(
        rendered.source.contains(
            "global::NeoDecompiler.Generated.OpCode.__NeoDecompilerAssertMessage(1 != 0, (string)(object)(2));"
        ),
        "{}",
        rendered.source
    );
}

#[test]
fn csharp_failure_statements_preserve_semantics_and_abort_warning() {
    let manifest = |name: &str| {
        ContractManifest::from_json_str(&format!(
            r#"{{
                "name": "{name}",
                "abi": {{ "methods": [{{
                    "name": "main", "parameters": [], "returntype": "Void", "offset": 0
                }}] }}
            }}"#
        ))
        .expect("manifest parsed")
    };
    let render = |name: &str, script: &[u8]| {
        render_csharp_with_coverage(&build_nef(script), Some(manifest(name)), true, false, true)
    };

    let assert_message = render("AssertMessage", &[0x11, 0x12, 0xE1, 0x40]);
    let throw = render("Throw", &[0x11, 0x3A]);
    let abort = render("Abort", &[0x38]);
    let abort_message = render("AbortMessage", &[0x12, 0xE0]);

    for rendered in [&assert_message, &throw, &abort, &abort_message] {
        assert_eq!(
            rendered
                .coverage
                .method(0, "main")
                .expect("main coverage")
                .backend,
            crate::decompiler::csharp::BodyBackend::Structured,
            "{}",
            rendered.source
        );
    }
    assert_eq!(
        assert_message
            .coverage
            .method(0, "main")
            .expect("assert coverage")
            .fidelity
            .status,
        crate::decompiler::cfg::method_body::Fidelity::Exact
    );
    assert_eq!(
        throw
            .coverage
            .method(0, "main")
            .expect("throw coverage")
            .fidelity
            .status,
        crate::decompiler::cfg::method_body::Fidelity::Exact
    );
    for rendered in [&abort, &abort_message] {
        assert_eq!(
            rendered
                .coverage
                .method(0, "main")
                .expect("abort coverage")
                .fidelity
                .status,
            crate::decompiler::cfg::method_body::Fidelity::Conservative
        );
        assert!(rendered.warnings.iter().any(|warning| {
            warning.contains("ABORT") && warning.contains("uncatchable VM abort")
        }));
    }
    assert!(
        assert_message
            .source
            .contains("__NeoDecompilerAssertMessage(1 != 0, (string)(object)(2));"),
        "{}",
        assert_message.source
    );
    assert!(
        throw
            .source
            .contains("throw new __NeoDecompilerVmException(1);"),
        "{}",
        throw.source
    );
    assert!(
        abort
            .source
            .contains("throw new InvalidOperationException();"),
        "{}",
        abort.source
    );
    assert!(
        abort_message
            .source
            .contains("throw new InvalidOperationException(Convert.ToString(2));"),
        "{}",
        abort_message.source
    );
}

#[test]
fn csharp_trace_failure_statements_preserve_semantics_and_abort_warning() {
    let manifest = |name: &str| {
        ContractManifest::from_json_str(&format!(
            r#"{{
                "name": "{name}",
                "abi": {{ "methods": [{{
                    "name": "main", "parameters": [], "returntype": "Void", "offset": 0
                }}] }}
            }}"#
        ))
        .expect("manifest parsed")
    };
    let render = |name: &str, script: &[u8]| {
        render_csharp_with_coverage(&build_nef(script), Some(manifest(name)), true, true, true)
    };

    let assertion = render("TraceAssert", &[0x11, 0x39, 0x40]);
    let assert_message = render("TraceAssertMessage", &[0x11, 0x12, 0xE1, 0x40]);
    let throw = render("TraceThrow", &[0x11, 0x3A]);
    let abort = render("TraceAbort", &[0x38]);
    let abort_message = render("TraceAbortMessage", &[0x12, 0xE0]);

    for rendered in [&assertion, &assert_message, &throw, &abort, &abort_message] {
        assert_eq!(
            rendered
                .coverage
                .method(0, "main")
                .expect("main coverage")
                .backend,
            crate::decompiler::csharp::BodyBackend::Structured,
            "{}",
            rendered.source
        );
    }
    assert!(
        assertion
            .source
            .contains("global::Neo.SmartContract.Framework.ExecutionEngine.Assert(1 != 0);"),
        "{}",
        assertion.source
    );
    assert!(
        assert_message
            .source
            .contains("__NeoDecompilerAssertMessage(1 != 0, (string)(object)(2));"),
        "{}",
        assert_message.source
    );
    assert!(
        throw
            .source
            .contains("throw new __NeoDecompilerVmException(1);"),
        "{}",
        throw.source
    );
    assert!(
        abort
            .source
            .contains("throw new InvalidOperationException();"),
        "{}",
        abort.source
    );
    assert!(
        abort_message
            .source
            .contains("throw new InvalidOperationException(Convert.ToString(2));"),
        "{}",
        abort_message.source
    );
    for rendered in [&abort, &abort_message] {
        assert!(rendered.warnings.iter().any(|warning| {
            warning.contains("ABORT") && warning.contains("uncatchable VM abort")
        }));
    }
}

#[test]
fn csharp_trace_assert_converts_numeric_local_to_boolean() {
    // INITSLOT 1,0; PUSH1; STLOC0; LDLOC0; ASSERT; RET.
    let nef = build_nef(&[0x57, 0x01, 0x00, 0x11, 0x70, 0x68, 0x39, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "TraceNumericAssert",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Void", "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered = render_csharp_with_coverage(&nef, Some(manifest), true, true, true);

    assert_eq!(
        rendered
            .coverage
            .method(0, "main")
            .expect("main coverage")
            .backend,
        crate::decompiler::csharp::BodyBackend::Structured
    );
    assert!(
        rendered.source.contains("BigInteger loc0 = 1;"),
        "{}",
        rendered.source
    );
    assert!(
        rendered
            .source
            .contains("global::Neo.SmartContract.Framework.ExecutionEngine.Assert(loc0 != 0);"),
        "{}",
        rendered.source
    );
    assert!(
        !rendered.source.contains("ExecutionEngine.Assert(loc0);"),
        "numeric locals are not C# boolean expressions: {}",
        rendered.source
    );
}

#[test]
fn csharp_trace_assert_converts_named_numeric_parameter_to_boolean() {
    // INITSLOT 0,1; LDARG0; ASSERT; RET.
    let nef = build_nef(&[0x57, 0x00, 0x01, 0x78, 0x39, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "TraceNumericParameterAssert",
            "abi": { "methods": [{
                "name": "main",
                "parameters": [{ "name": "amount", "type": "Integer" }],
                "returntype": "Void",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    for typed_declarations in [false, true] {
        let rendered = render_csharp_with_coverage(
            &nef,
            Some(manifest.clone()),
            true,
            true,
            typed_declarations,
        );

        assert_eq!(
            rendered
                .coverage
                .method(0, "main")
                .expect("main coverage")
                .backend,
            crate::decompiler::csharp::BodyBackend::Structured
        );
        assert!(
            rendered.source.contains(
                "global::Neo.SmartContract.Framework.ExecutionEngine.Assert(amount != 0);"
            ),
            "{}",
            rendered.source
        );
        assert!(
            !rendered
                .source
                .contains("ExecutionEngine.Assert((bool)(object)(amount));"),
            "manifest numeric parameters should not depend on erased C# casts: {}",
            rendered.source
        );
    }
}

#[test]
fn csharp_trace_assert_prefers_emitted_parameter_name_over_raw_slot_syntax() {
    // INITSLOT 0,2; LDARG0; ASSERT; RET. The first parameter is deliberately
    // named `arg1`; name lookup must still use slot 0's Boolean type.
    let nef = build_nef(&[0x57, 0x00, 0x02, 0x78, 0x39, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "ArgumentNamePrecedence",
            "abi": { "methods": [{
                "name": "main",
                "parameters": [
                    { "name": "arg1", "type": "Boolean" },
                    { "name": "amount", "type": "Integer" }
                ],
                "returntype": "Void",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered = render_csharp_with_coverage(&nef, Some(manifest), true, true, true);

    assert!(
        rendered
            .source
            .contains("global::Neo.SmartContract.Framework.ExecutionEngine.Assert(arg1);"),
        "emitted parameter names must take precedence over raw argN syntax: {}",
        rendered.source
    );
    assert!(
        !rendered.source.contains("arg1 != 0"),
        "a Boolean slot-0 parameter must not inherit slot 1's Integer type: {}",
        rendered.source
    );
}

#[test]
fn csharp_constant_pack_is_structured_while_trace_mode_selects_legacy() {
    let pack_nef = build_nef(&[0x12, 0x11, 0x12, 0xC0, 0x40]);
    let pack = render_csharp_with_coverage(&pack_nef, None, true, false, true);
    assert_eq!(
        pack.coverage
            .method(0, "ScriptEntry")
            .expect("pack coverage")
            .backend,
        crate::decompiler::csharp::BodyBackend::Structured
    );
    assert!(
        pack.source.contains("new object[] { 1, 2 }"),
        "constant PACK must render a compile-oriented array literal: {}",
        pack.source
    );

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let nef = fs::read(root.join("TestingArtifacts/edgecases/multi/MultiMethod.nef"))
        .expect("MultiMethod NEF");
    let manifest =
        fs::read_to_string(root.join("TestingArtifacts/edgecases/multi/MultiMethod.manifest.json"))
            .expect("MultiMethod manifest");
    let manifest = ContractManifest::from_json_str(&manifest).expect("manifest parsed");
    let traced = render_csharp_with_coverage(&nef, Some(manifest), true, true, true);
    assert_eq!(
        traced
            .coverage
            .method(6, "helper")
            .expect("helper coverage")
            .backend,
        crate::decompiler::csharp::BodyBackend::Structured
    );
    assert!(traced.source.contains("// 0006:"), "{}", traced.source);
}

#[test]
fn csharp_structures_pack_families_and_constant_unpack() {
    let cases = [
        (
            vec![
                crate::instruction::OpCode::Push2.byte(),
                crate::instruction::OpCode::Push1.byte(),
                crate::instruction::OpCode::Push2.byte(),
                crate::instruction::OpCode::Packstruct.byte(),
                crate::instruction::OpCode::Ret.byte(),
            ],
            "new object[] { 1, 2 }",
        ),
        (
            vec![
                crate::instruction::OpCode::Push4.byte(),
                crate::instruction::OpCode::Push3.byte(),
                crate::instruction::OpCode::Push2.byte(),
                crate::instruction::OpCode::Push1.byte(),
                crate::instruction::OpCode::Push2.byte(),
                crate::instruction::OpCode::Packmap.byte(),
                crate::instruction::OpCode::Ret.byte(),
            ],
            "new Map<object, object> { [1] = 2, [3] = 4 }",
        ),
        (
            vec![
                crate::instruction::OpCode::Push2.byte(),
                crate::instruction::OpCode::Push1.byte(),
                crate::instruction::OpCode::Push2.byte(),
                crate::instruction::OpCode::Pack.byte(),
                crate::instruction::OpCode::Unpack.byte(),
                crate::instruction::OpCode::Drop.byte(),
                crate::instruction::OpCode::Sub.byte(),
                crate::instruction::OpCode::Ret.byte(),
            ],
            "return 1;",
        ),
    ];

    for (script, expected) in cases {
        let rendered = render_csharp_with_coverage(&build_nef(&script), None, true, false, true);
        let coverage = rendered
            .coverage
            .method(0, "ScriptEntry")
            .expect("entry coverage");

        assert_eq!(
            coverage.backend,
            crate::decompiler::csharp::BodyBackend::Structured,
            "{coverage:#?}"
        );
        assert!(
            rendered.source.contains(expected),
            "expected {expected:?}:\n{}",
            rendered.source
        );
        assert!(!rendered.source.contains('?'), "{}", rendered.source);
    }
}

#[test]
fn csharp_structures_printable_raw_and_wide_literals() {
    let mut printable = vec![crate::instruction::OpCode::Pushdata1.byte(), 5];
    printable.extend_from_slice(b"hello");
    printable.push(crate::instruction::OpCode::Ret.byte());

    let raw = vec![
        crate::instruction::OpCode::Pushdata1.byte(),
        2,
        0x00,
        0xFF,
        crate::instruction::OpCode::Ret.byte(),
    ];

    let mut wide = vec![crate::instruction::OpCode::Pushint256.byte()];
    wide.extend([0xFF; 32]);
    wide.push(crate::instruction::OpCode::Ret.byte());

    for (script, expected) in [
        (printable, "\"hello\""),
        (raw, "(ByteString)new byte[] { 0x00, 0xFF }"),
        (wide, "BigInteger.Parse(\"-1\")"),
    ] {
        let rendered = render_csharp_with_coverage(&build_nef(&script), None, true, false, true);
        let coverage = rendered
            .coverage
            .method(0, "ScriptEntry")
            .expect("entry coverage");

        assert_eq!(
            coverage.backend,
            crate::decompiler::csharp::BodyBackend::Structured,
            "{coverage:#?}"
        );
        assert!(
            rendered.source.contains(expected),
            "expected {expected:?}:\n{}",
            rendered.source
        );
    }
}

#[test]
fn csharp_type_tag_operands_use_structured_renderer() {
    let cases = [
        (
            "ConvertTag",
            crate::instruction::OpCode::Convert,
            "global::NeoDecompiler.Generated.ConvertTag.__NeoDecompilerConvertInteger(1)",
        ),
        (
            "IsTypeTag",
            crate::instruction::OpCode::Istype,
            "global::NeoDecompiler.Generated.IsTypeTag.__NeoDecompilerIsTypeInteger(1)",
        ),
        (
            "NewArrayTag",
            crate::instruction::OpCode::NewarrayT,
            "new BigInteger[(int)(1)]",
        ),
    ];

    for (contract_name, opcode, expected) in cases {
        let script = [
            crate::instruction::OpCode::Push1.byte(),
            opcode.byte(),
            0x21,
            crate::instruction::OpCode::Ret.byte(),
        ];
        let manifest = ContractManifest::from_json_str(&format!(
            r#"{{
                "name": "{contract_name}",
                "abi": {{ "methods": [{{
                    "name": "main", "parameters": [], "returntype": "Any", "offset": 0
                }}] }}
            }}"#
        ))
        .expect("manifest parsed");

        let rendered =
            render_csharp_with_coverage(&build_nef(&script), Some(manifest), true, false, true);

        assert_eq!(
            rendered
                .coverage
                .method(0, "main")
                .expect("main coverage")
                .backend,
            crate::decompiler::csharp::BodyBackend::Structured,
            "{opcode:?}: {:#?}",
            rendered.coverage
        );
        assert!(
            rendered.source.contains(expected),
            "{opcode:?} must retain its operand tag:\n{}",
            rendered.source
        );
        if matches!(
            opcode,
            crate::instruction::OpCode::Convert | crate::instruction::OpCode::Istype
        ) {
            let helper = match opcode {
                crate::instruction::OpCode::Convert => {
                    "[global::Neo.SmartContract.Framework.Attributes.OpCode(global::Neo.SmartContract.Framework.OpCode.CONVERT, \"21\")]\n        private static extern BigInteger __NeoDecompilerConvertInteger(object value);"
                }
                crate::instruction::OpCode::Istype => {
                    "[global::Neo.SmartContract.Framework.Attributes.OpCode(global::Neo.SmartContract.Framework.OpCode.ISTYPE, \"21\")]\n        private static extern bool __NeoDecompilerIsTypeInteger(object value);"
                }
                _ => unreachable!("matched tagged opcode above"),
            };
            assert!(
                rendered.source.contains(helper),
                "{opcode:?} must emit a direct opcode helper:\n{}",
                rendered.source
            );
            assert!(
                !rendered.source.contains("Runtime.LoadScript"),
                "tagged opcodes must not acquire AllowCall:\n{}",
                rendered.source
            );
        }
        if opcode == crate::instruction::OpCode::NewarrayT {
            assert!(
                rendered
                    .source
                    .contains("BigInteger[] t_1 = new BigInteger[(int)(1)];"),
                "NEWARRAY_T needs a compile-oriented typed declaration:\n{}",
                rendered.source
            );
        }
    }
}

#[test]
fn csharp_type_tag_helper_avoids_contract_member_collisions() {
    let script = [
        crate::instruction::OpCode::Push1.byte(),
        crate::instruction::OpCode::Convert.byte(),
        0x21,
        crate::instruction::OpCode::Ret.byte(),
    ];
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "TaggedHelperCollision",
            "abi": { "methods": [{
                "name": "__NeoDecompilerConvertInteger",
                "parameters": [],
                "returntype": "Any",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered =
        render_csharp_with_coverage(&build_nef(&script), Some(manifest), true, false, true);

    assert!(
        rendered.source.contains(
            "private static extern BigInteger __NeoDecompilerConvertInteger_1(object value);"
        ),
        "tagged helper must avoid ABI member names:\n{}",
        rendered.source
    );
    assert!(
        rendered.source.contains(
            "global::NeoDecompiler.Generated.TaggedHelperCollision.__NeoDecompilerConvertInteger_1(1)"
        ),
        "tagged helper call must use the collision-safe declaration name:\n{}",
        rendered.source
    );
}

#[test]
fn csharp_unpack_packstruct_helper_preserves_opcode_order_and_avoids_collisions() {
    let script = [
        crate::instruction::OpCode::Push1.byte(),
        crate::instruction::OpCode::Unpack.byte(),
        crate::instruction::OpCode::Packstruct.byte(),
        crate::instruction::OpCode::Ret.byte(),
    ];
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "UnpackPackStructCollision",
            "abi": { "methods": [{
                "name": "__NeoDecompilerUnpackPackStruct",
                "parameters": [],
                "returntype": "Any",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered =
        render_csharp_with_coverage(&build_nef(&script), Some(manifest), true, false, true);

    let helper = "[global::Neo.SmartContract.Framework.Attributes.OpCode(global::Neo.SmartContract.Framework.OpCode.UNPACK)]\n        [global::Neo.SmartContract.Framework.Attributes.OpCode(global::Neo.SmartContract.Framework.OpCode.PACKSTRUCT)]\n        private static extern object[] __NeoDecompilerUnpackPackStruct_1(object value);";
    assert!(
        rendered.source.contains(helper),
        "clone helper must preserve opcode order and avoid ABI member names:\n{}",
        rendered.source
    );
    assert!(
        rendered.source.contains(
            "global::NeoDecompiler.Generated.UnpackPackStructCollision.__NeoDecompilerUnpackPackStruct_1(1)"
        ),
        "clone call must use the collision-safe declaration name:\n{}",
        rendered.source
    );
}

#[test]
fn csharp_bare_throw_helper_preserves_the_opcode_and_avoids_collisions() {
    let script = [
        crate::instruction::OpCode::Push1.byte(),
        crate::instruction::OpCode::Drop.byte(),
        crate::instruction::OpCode::Throw.byte(),
    ];
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "BareThrowCollision",
            "abi": { "methods": [{
                "name": "__NeoDecompilerBareThrow",
                "parameters": [],
                "returntype": "Void",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered =
        render_csharp_with_coverage(&build_nef(&script), Some(manifest), true, false, true);

    assert!(
        rendered.source.contains(
            "[global::Neo.SmartContract.Framework.Attributes.OpCode(global::Neo.SmartContract.Framework.OpCode.THROW)]\n        private static extern void __NeoDecompilerBareThrow_1();"
        ),
        "bare-throw helper must preserve the opcode and avoid ABI member names:\n{}",
        rendered.source
    );
    assert!(
        rendered.source.contains(
            "global::NeoDecompiler.Generated.BareThrowCollision.__NeoDecompilerBareThrow_1();"
        ),
        "bare-throw call must use the collision-safe declaration name:\n{}",
        rendered.source
    );
}

#[test]
fn csharp_static_initializer_keeps_adjacent_static_and_local_slot_prologues_together() {
    let script = [
        crate::instruction::OpCode::Ldsfld0.byte(),
        crate::instruction::OpCode::Ret.byte(),
        crate::instruction::OpCode::Initsslot.byte(),
        1,
        crate::instruction::OpCode::Initslot.byte(),
        0,
        0,
        crate::instruction::OpCode::Push3.byte(),
        crate::instruction::OpCode::Stsfld0.byte(),
        crate::instruction::OpCode::Ret.byte(),
    ];
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "StaticInitializer",
            "abi": { "methods": [
                {
                    "name": "testStatic",
                    "parameters": [],
                    "returntype": "Integer",
                    "offset": 0
                },
                {
                    "name": "_initialize",
                    "parameters": [],
                    "returntype": "Void",
                    "offset": 2
                }
            ] }
        }"#,
    )
    .expect("manifest parsed");

    let rendered =
        render_csharp_with_coverage(&build_nef(&script), Some(manifest), true, false, true);

    assert!(
        rendered.source.contains("public static void _initialize()"),
        "manifest initializer must be emitted:\n{}",
        rendered.source
    );
    assert!(
        rendered.source.contains("static0 = 3;"),
        "initializer body must retain its static assignment:\n{}",
        rendered.source
    );
    assert!(
        !rendered.source.contains("sub_0x0004"),
        "the local-slot prologue must not create a detached helper:\n{}",
        rendered.source
    );
    assert_eq!(
        rendered
            .coverage
            .method(2, "_initialize")
            .expect("initializer coverage")
            .fidelity
            .status,
        crate::decompiler::cfg::method_body::Fidelity::Exact
    );
}

#[test]
fn csharp_type_tags_preserve_bytestring_and_struct_identity() {
    let cases = [
        (
            vec![
                crate::instruction::OpCode::Pushdata1.byte(),
                1,
                0xFF,
                crate::instruction::OpCode::Istype.byte(),
                0x28,
                crate::instruction::OpCode::Ret.byte(),
            ],
            [
                "(ByteString)new byte[] { 0xFF }",
                "__NeoDecompilerIsTypeByteString((ByteString)new byte[] { 0xFF })",
            ],
        ),
        (
            vec![
                crate::instruction::OpCode::Push2.byte(),
                crate::instruction::OpCode::Push1.byte(),
                crate::instruction::OpCode::Push2.byte(),
                crate::instruction::OpCode::Packstruct.byte(),
                crate::instruction::OpCode::Istype.byte(),
                0x40,
                crate::instruction::OpCode::Ret.byte(),
            ],
            [
                "__NeoDecompilerConvertStruct(new object[] { 1, 2 })",
                "__NeoDecompilerIsTypeArray(",
            ],
        ),
    ];

    for (script, expected_fragments) in cases {
        let rendered = render_csharp_with_coverage(&build_nef(&script), None, true, false, true);
        let coverage = rendered
            .coverage
            .method(0, "ScriptEntry")
            .expect("entry coverage");

        assert_eq!(
            coverage.backend,
            crate::decompiler::csharp::BodyBackend::Structured,
            "{coverage:#?}"
        );
        for expected in expected_fragments {
            assert!(
                rendered.source.contains(expected),
                "expected {expected:?}:\n{}",
                rendered.source
            );
        }
        assert!(
            !rendered.source.contains("Runtime.LoadScript"),
            "typed collection operations must not acquire AllowCall:\n{}",
            rendered.source
        );
    }
}

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

#[test]
fn legacy_statement_to_csharp_converts_known_forms() {
    assert_eq!(legacy_statement_to_csharp("   "), "");
    assert_eq!(legacy_statement_to_csharp("// note"), "// note");
    assert_eq!(legacy_statement_to_csharp("let x = 1;"), "var x = 1;");
    // Helper rewrites must apply inside `let` initialisers too —
    // earlier the `let` branch early-returned before
    // `legacy_expression_to_csharp` ran, so `let t0 = min(x, y);` came
    // out as `var t0 = min(x, y);` (uncompilable).
    assert_eq!(
        legacy_statement_to_csharp("let t0 = min(x, y);"),
        "var t0 = BigInteger.Min(x, y);"
    );
    assert_eq!(
        legacy_statement_to_csharp("let t0 = is_null(loc0);"),
        "var t0 = (loc0 is null);"
    );
    assert_eq!(
        legacy_statement_to_csharp("let t0 = a cat b;"),
        "var t0 = a + b;"
    );
    // Helper rewrites must also apply inside throw / abort / assert
    // operands. Same bug class as the `let` branch fix — these
    // branches were extracting their bodies but not running them
    // through the expression rewriter.
    // Payloads use the same explicit conversion as structured rendering.
    assert_eq!(
        legacy_statement_to_csharp("throw(min(a, b));"),
        "throw new Exception(Convert.ToString(BigInteger.Min(a, b)));"
    );
    assert_eq!(
        legacy_statement_to_csharp("abort(\"err\" cat code);"),
        "throw new InvalidOperationException(Convert.ToString(\"err\" + code));"
    );
    assert_eq!(
        legacy_statement_to_csharp("assert(is_null(loc0));"),
        "global::Neo.SmartContract.Framework.ExecutionEngine.Assert((bool)(object)((loc0 is null)));"
    );
    // Assert messages use explicit conversion regardless of source type.
    assert_eq!(
        legacy_statement_to_csharp("assert(min(a, b) > 0, \"e\" cat code);"),
        "__NeoDecompilerAssertMessage((bool)(object)(BigInteger.Min(a, b) > 0), (string)(object)(\"e\" + code));"
    );
    assert_eq!(
        legacy_statement_to_csharp("assert(x > 0, code);"),
        "__NeoDecompilerAssertMessage((bool)(object)(x > 0), (string)(object)(code));"
    );
    assert_eq!(legacy_statement_to_csharp("if t0 {"), "if (t0) {");
    assert_eq!(legacy_statement_to_csharp("while t1 {"), "while (t1) {");
    assert_eq!(legacy_statement_to_csharp("loop {"), "while (true) {");
    assert_eq!(
        legacy_statement_to_csharp("else if loc0 < 3 {"),
        "else if (loc0 < 3) {"
    );
    assert_eq!(
        legacy_statement_to_csharp("} else if loc0 == 1 {"),
        "} else if (loc0 == 1) {"
    );
    assert_eq!(
        legacy_statement_to_csharp("for (let i = 0; i < 3; i++) {"),
        "for (var i = 0; i < 3; i++) {"
    );
    assert_eq!(
        legacy_statement_to_csharp("leave label_0x0010;"),
        "goto label_0x0010;"
    );
    // CAT operator (high-level pseudocode) → C# `+`. The translation
    // only fires for ` cat ` tokens outside string literals.
    assert_eq!(
        legacy_statement_to_csharp("return \"b:\" cat addr;"),
        "return \"b:\" + addr;"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = a cat b cat c;"),
        "var x = a + b + c;"
    );
    assert_eq!(
        legacy_statement_to_csharp("var msg = \"says cat ok\";"),
        "var msg = \"says cat ok\";"
    );
    // `throw(value);` (high-level pseudocode for NEO's THROW opcode)
    // becomes `throw new Exception(value);` in C# — NEO accepts any
    // stack value, but C# requires an `Exception`.
    assert_eq!(
        legacy_statement_to_csharp("throw(\"oops\");"),
        "throw new Exception(Convert.ToString(\"oops\"));"
    );
    // Non-string-literal identifiers use the same explicit coercion.
    assert_eq!(
        legacy_statement_to_csharp("throw(error_msg);"),
        "throw new Exception(Convert.ToString(error_msg));"
    );
    // ABORT / ABORTMSG stay visibly distinct from catchable THROW.
    assert_eq!(
        legacy_statement_to_csharp("abort();"),
        "throw new InvalidOperationException();"
    );
    assert_eq!(
        legacy_statement_to_csharp("abort(\"fatal\");"),
        "throw new InvalidOperationException(Convert.ToString(\"fatal\"));"
    );
    // Identifier operand — same wrapping rule as `throw(error_msg)`
    // since we don't know its static type.
    assert_eq!(
        legacy_statement_to_csharp("abort(reason);"),
        "throw new InvalidOperationException(Convert.ToString(reason));"
    );
    // ASSERT uses the framework API. ASSERTMSG uses a local opcode helper so
    // message validation remains eager instead of the framework's lazy JMPIF
    // + ABORTMSG lowering.
    assert_eq!(
        legacy_statement_to_csharp("assert(x > 0);"),
        "global::Neo.SmartContract.Framework.ExecutionEngine.Assert((bool)(object)(x > 0));"
    );
    assert_eq!(
        legacy_statement_to_csharp("assert(x > 0, \"must be positive\");"),
        "__NeoDecompilerAssertMessage((bool)(object)(x > 0), (string)(object)(\"must be positive\"));"
    );
    assert_eq!(
        legacy_statement_to_csharp("assert(1, 2);"),
        "__NeoDecompilerAssertMessage(1 != 0, (string)(object)(2));"
    );
    // Don't be fooled by commas inside the condition expression.
    assert_eq!(
        legacy_statement_to_csharp("assert(foo(a, b));"),
        "global::Neo.SmartContract.Framework.ExecutionEngine.Assert((bool)(object)(foo(a, b)));"
    );
    assert_eq!(
        legacy_statement_to_csharp("assert(null);"),
        "global::Neo.SmartContract.Framework.ExecutionEngine.Assert(false);"
    );
    // NEO arithmetic helpers — the high-level lift emits `abs/min/max/pow`
    // as bare function calls, but C# has no `abs` etc. in scope. Rewrite
    // to `BigInteger.X(...)`. For `pow`, the second argument must be
    // `int` per `BigInteger.Pow`'s signature.
    assert_eq!(
        legacy_statement_to_csharp("var x = abs(loc0);"),
        "var x = BigInteger.Abs(loc0);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = min(a, b);"),
        "var x = BigInteger.Min(a, b);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = max(a, b);"),
        "var x = BigInteger.Max(a, b);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = pow(base, exp);"),
        "var x = BigInteger.Pow(base, (int)(exp));"
    );
    // Literal exponent skips the redundant `(int)` cast — same idea
    // as `wrap_int_cast_unless_literal`. `pow(2, 8)` lifts cleanly to
    // `BigInteger.Pow(2, 8)` rather than `BigInteger.Pow(2, (int)(8))`.
    assert_eq!(
        legacy_statement_to_csharp("var x = pow(2, 8);"),
        "var x = BigInteger.Pow(2, 8);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = left(buf, 4);"),
        "var x = Helper.Left(buf, 4);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = substr(buf, 0, 16);"),
        "var x = Helper.Substr(buf, 0, 16);"
    );
    // Identifier-boundary respect: `mypow(x)` is NOT `pow(x)`.
    assert_eq!(
        legacy_statement_to_csharp("var x = mypow(2);"),
        "var x = mypow(2);"
    );
    // String-literal preservation: `"min(a)"` inside a string stays
    // verbatim.
    assert_eq!(
        legacy_statement_to_csharp("var x = \"min(a, b)\";"),
        "var x = \"min(a, b)\";"
    );
    // Nested helpers compose: `max(abs(a), b)` → `BigInteger.Max(BigInteger.Abs(a), b)`.
    assert_eq!(
        legacy_statement_to_csharp("var x = max(abs(a), b);"),
        "var x = BigInteger.Max(BigInteger.Abs(a), b);"
    );
    // Extended NEO arithmetic / buffer helpers — `BigInteger.X` for
    // ones .NET provides directly, `Helper.X` (Neo SmartContract
    // Framework) for the rest. Args at int-typed positions get an
    // `(int)(...)` cast so the C# overload signature matches.
    assert_eq!(
        legacy_statement_to_csharp("var x = sign(loc0);"),
        "var x = Helper.Sign(loc0);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = sqrt(loc0);"),
        "var x = Helper.Sqrt(loc0);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = modmul(a, b, m);"),
        "var x = Helper.ModMul(a, b, m);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = modpow(b, e, m);"),
        "var x = BigInteger.ModPow(b, e, m);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = within(v, lo, hi);"),
        "var x = Helper.Within(v, lo, hi);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = left(buf, n);"),
        "var x = Helper.Left(buf, (int)(n));"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = right(buf, n);"),
        "var x = Helper.Right(buf, (int)(n));"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = substr(buf, start, len);"),
        "var x = Helper.Substr(buf, (int)(start), (int)(len));"
    );
    // `is_null(x)` is a unary check, not a function call — it lifts
    // to the idiomatic C# pattern `(x is null)` instead of trying to
    // resolve a (non-existent) `IsNull` helper on the framework.
    assert_eq!(
        legacy_statement_to_csharp("if is_null(loc0) {"),
        "if ((loc0 is null)) {"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = is_null(loc0);"),
        "var x = (loc0 is null);"
    );
    // Nested into another helper: `if (!is_null(x))` style usages.
    assert_eq!(
        legacy_statement_to_csharp("var y = !is_null(loc0);"),
        "var y = !(loc0 is null);"
    );
    // Identifier-boundary respect: `assert_is_null(x)` must NOT pick
    // up the `is_null` rewrite (it's a different identifier).
    assert_eq!(
        legacy_statement_to_csharp("var x = my_is_null(loc0);"),
        "var x = my_is_null(loc0);"
    );
    // Empty collection constructors lifted from NEWMAP / NEWARRAY0 /
    // NEWSTRUCT0 — the lift emits `Map()`, `[]`, `Struct()` which
    // don't compile as-is. Rewrite to explicit `new` forms with
    // best-effort type defaults (`object` for Map's generic args
    // since we don't have key/value type info; `object[0]` for the
    // bare-literal array case).
    assert_eq!(
        legacy_statement_to_csharp("var t0 = Map();"),
        "var t0 = new Map<object, object>();"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = [];"),
        "var t0 = new object[0];"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = Struct();"),
        "var t0 = new Struct();"
    );
    // Identifier-boundary respect — a user-named `MyMap()` factory
    // must NOT be rewritten to `new MyMap<...>()`.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = MyMap();"),
        "var t0 = MyMap();"
    );
    // String-literal preservation — `"Map()"` inside a quoted
    // string stays verbatim.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = \"Map()\";"),
        "var t0 = \"Map()\";"
    );
    // Size-operand constructors lifted from NEWBUFFER / NEWARRAY
    // — `new_buffer(n)` and `new_array(n)` aren't valid C#
    // identifiers; rewrite to explicit `new byte[...]` /
    // `new object[...]`. The size operand needs a defensive
    // `(int)` cast for any expression that could carry BigInteger
    // semantics, but bare integer literals are unambiguously `int`
    // to the C# parser, so `wrap_int_cast_unless_literal` skips the
    // cast for them — yielding `new object[3]` instead of the noisier
    // `new object[(int)(3)]`. Variable / expression operands still
    // get the cast.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = new_buffer(8);"),
        "var t0 = new byte[8];"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = new_buffer(loc0);"),
        "var t0 = new byte[(int)(loc0)];"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = new_array(3);"),
        "var t0 = new object[3];"
    );
    // Negative literals also pass through without cast. Negative
    // sizes don't make sense for `new T[]` but the cast wouldn't
    // help anyway — `new T[-3]` and `new T[(int)(-3)]` are both
    // accepted by the C# compiler and reject at runtime alike.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = new_array(-3);"),
        "var t0 = new object[-3];"
    );
    // Identifier-boundary respect — `my_new_buffer(8)` is NOT the
    // NEWBUFFER lift output.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = my_new_buffer(8);"),
        "var t0 = my_new_buffer(8);"
    );
    // CONVERT lifts (`convert_to_bool` / `convert_to_integer` /
    // `convert_to_bytestring` / `convert_to_buffer`) — rewrite to
    // explicit C# casts.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = convert_to_bool(loc0);"),
        "var t0 = (bool)(loc0);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = convert_to_integer(loc0);"),
        "var t0 = (BigInteger)(loc0);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = convert_to_bytestring(loc0);"),
        "var t0 = (ByteString)(loc0);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = convert_to_buffer(loc0);"),
        "var t0 = (byte[])(loc0);"
    );
    // ISTYPE lifts — rewrite to C# pattern matches.
    assert_eq!(
        legacy_statement_to_csharp("if is_type_bool(loc0) {"),
        "if ((loc0 is bool)) {"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = is_type_integer(loc0);"),
        "var t0 = (loc0 is BigInteger);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = is_type_bytestring(loc0);"),
        "var t0 = (loc0 is ByteString);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = is_type_buffer(loc0);"),
        "var t0 = (loc0 is byte[]);"
    );
    // The other CONVERT / ISTYPE variants (any, pointer, array,
    // struct, map, interopinterface) deliberately keep the lifted
    // form — silently rewriting them would require type info the
    // lift doesn't supply. Leave a clear hint that the user has to
    // pick the right cast.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = convert_to_array(loc0);"),
        "var t0 = convert_to_array(loc0);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = is_type_map(loc0);"),
        "var t0 = is_type_map(loc0);"
    );
    // Collection helpers — `clear_items(c)`, `remove_item(c, k)`,
    // `keys(m)`, `values(m)`, `reverse_items(arr)` are NEO-flavoured
    // identifiers that don't compile. Rewrite to standard
    // .NET / Neo Map accessors.
    assert_eq!(
        legacy_statement_to_csharp("clear_items(loc0);"),
        "loc0.Clear();"
    );
    assert_eq!(
        legacy_statement_to_csharp("remove_item(loc0, key);"),
        "loc0.Remove(key);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = keys(loc0);"),
        "var t0 = loc0.Keys;"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = values(loc0);"),
        "var t0 = loc0.Values;"
    );
    assert_eq!(
        legacy_statement_to_csharp("reverse_items(loc0);"),
        "loc0.Reverse();"
    );
    // Identifier-boundary respect — `my_keys(loc0)` is NOT KEYS.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = my_keys(loc0);"),
        "var t0 = my_keys(loc0);"
    );
    // APPEND lift — `append(arr, item)` → `arr.Add(item)`.
    assert_eq!(
        legacy_statement_to_csharp("append(loc0, 42);"),
        "loc0.Add(42);"
    );
    // HASKEY lift — `has_key(c, k)` → `c.ContainsKey(k)`.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = has_key(loc0, key);"),
        "var t0 = loc0.ContainsKey(key);"
    );
    assert_eq!(
        legacy_statement_to_csharp("if has_key(loc0, key) {"),
        "if (loc0.ContainsKey(key)) {"
    );
}

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

#[test]
fn legacy_statement_to_csharp_does_not_panic_on_degenerate_headers() {
    // Regression: empty-condition `if {` / `while {` headers used raw byte
    // slicing (`trimmed[3..len-2]`) which panicked with begin > end. They must
    // be handled without panicking.
    let _ = legacy_statement_to_csharp("if {");
    let _ = legacy_statement_to_csharp("while {");
    // Regression (adversarial): a multibyte UTF-8 character outside a string
    // literal (e.g. a method-token name) previously panicked in the helper
    // rewriter via `&line[i..]` landing mid-character. It must be preserved.
    assert_eq!(legacy_statement_to_csharp("é(t0);"), "é(t0);");
    assert_eq!(
        legacy_statement_to_csharp("let t0 = \"café\";"),
        "var t0 = \"café\";"
    );
    let _ = legacy_statement_to_csharp("naïve(abs(x));");
    // Well-formed headers still convert.
    assert_eq!(
        legacy_statement_to_csharp("if loc0 == 1 {"),
        "if (loc0 == 1) {"
    );
    assert_eq!(
        legacy_statement_to_csharp("while loc0 < 3 {"),
        "while (loc0 < 3) {"
    );
}

#[test]
fn csharpize_nested_helper_calls_in_cast_path_helpers() {
    // pow/left/right/substr take the int-cast argument path; nested NEO helper
    // calls in their arguments must still be rewritten to compilable C# rather
    // than emitted verbatim.
    assert_eq!(
        legacy_statement_to_csharp("let t0 = pow(abs(x), 2);"),
        "var t0 = BigInteger.Pow(BigInteger.Abs(x), 2);"
    );
}

#[test]
fn legacy_expression_to_csharp_preserves_multibyte_in_cat_path() {
    // Regression (adversarial recheck): rewrite_cat_operator runs first in
    // legacy_expression_to_csharp and previously mangled multibyte UTF-8 (push b as char
    // re-encodes as Latin-1) when the line contained ` cat `.
    assert_eq!(
        legacy_statement_to_csharp("let t0 = café cat x;"),
        "var t0 = café + x;"
    );
    assert_eq!(
        legacy_statement_to_csharp("let t0 = \"naïve\" cat y;"),
        "var t0 = \"naïve\" + y;"
    );
}

#[test]
fn csharp_escapes_control_chars_in_pushdata_string_literal() {
    // PUSHDATA1 "a\nb" (a, raw newline, b) + RET. A raw newline inside a C#
    // string constant is error CS1010, so the lifted literal must escape it.
    let nef_bytes = build_nef(&[0x0C, 0x03, b'a', b'\n', b'b', 0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");
    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains(r#""a\nb""#),
        "newline in PUSHDATA string must be escaped as \\n: {csharp}"
    );
    assert!(
        !csharp.contains("a\nb"),
        "a raw newline must not appear inside the C# string literal: {csharp}"
    );
}

#[test]
fn csharp_non_void_method_with_empty_body_throws_not_implemented() {
    // `prologue` spans [0,3) = INITSLOT only, which lifts to no statements.
    // A non-void (Integer) return with no body is C# error CS0161, so the
    // renderer must emit a throwing stub rather than a bare comment.
    let nef_bytes = build_nef(&[0x57, 0x00, 0x00, 0x11, 0x12, 0x9E, 0x40, 0x20, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"{"name":"Demo","abi":{"methods":[
            {"name":"prologue","returntype":"Integer","offset":0,"parameters":[],"safe":false},
            {"name":"body","returntype":"Integer","offset":3,"parameters":[],"safe":false}
        ],"events":[]}}"#,
    )
    .expect("manifest parsed");
    // Clean mode (trace comments off) is the user-facing compilable C# output —
    // the mode the CLI `decompile --format csharp` emits. There INITSLOT lifts
    // to no statements, so `prologue`'s body is empty.
    let decompilation = Decompiler::new()
        .with_trace_comments(false)
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");
    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("BigInteger prologue()"),
        "prologue should render with its declared return type: {csharp}"
    );
    assert!(
        csharp.contains("throw new NotImplementedException();"),
        "non-void method with an empty lifted body must throw, not emit a bare comment: {csharp}"
    );
}

#[test]
fn csharp_void_event_parameter_renders_as_object_not_void() {
    // An event arg typed `Void` previously rendered `Action<void>`
    // (C# error CS1547: void cannot be a type argument). It must map to
    // `object` in non-return position.
    let nef_bytes = build_nef(&[0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"{"name":"Ev","abi":{"methods":[
            {"name":"main","returntype":"Void","offset":0,"parameters":[],"safe":false}
        ],"events":[{"name":"Boom","parameters":[{"name":"x","type":"Void"}]}]}}"#,
    )
    .expect("manifest parsed");
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");
    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("Action<object> Boom"),
        "void event parameter must render as object: {csharp}"
    );
    assert!(
        !csharp.contains("Action<void>"),
        "void must never appear as a generic type argument: {csharp}"
    );
}

#[test]
fn csharp_wraps_only_oversized_integer_literals() {
    // PUSHINT256 = 1<<200 (a 61-digit value, > ulong.MaxValue) + RET. A bare
    // decimal literal above ulong.MaxValue is C# error CS1021, so it must
    // become BigInteger.Parse("…").
    let mut script = vec![0x05];
    let mut operand = vec![0u8; 32];
    operand[25] = 0x01; // bit 200
    script.extend_from_slice(&operand);
    script.push(0x40);
    let nef_bytes = build_nef(&script);
    let csharp = Decompiler::new()
        .with_inline_single_use_temps(true)
        .with_trace_comments(false)
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds")
        .csharp
        .expect("csharp output");
    assert!(
        csharp.contains(
            r#"BigInteger.Parse("1606938044258990275541962092341162602522202993782792835301376")"#
        ),
        "oversized integer literal must be wrapped in BigInteger.Parse: {csharp}"
    );

    // Boundary + non-decimal cases via the statement rewriter:
    // u64::MAX fits in a C# `ulong` literal — leave it bare.
    assert_eq!(
        legacy_statement_to_csharp("return 18446744073709551615;"),
        "return 18446744073709551615;"
    );
    // u64::MAX + 1 exceeds `ulong` — wrap it.
    assert_eq!(
        legacy_statement_to_csharp("return 18446744073709551616;"),
        r#"return BigInteger.Parse("18446744073709551616");"#
    );
    // Small literals, hex (syscall hashes), and label identifiers are untouched.
    assert_eq!(legacy_statement_to_csharp("return 42;"), "return 42;");
    assert!(
        !legacy_statement_to_csharp("let t0 = syscall(0xDEADBEEF);").contains("BigInteger.Parse")
    );
    assert!(!legacy_statement_to_csharp("goto label_0x0010;").contains("BigInteger.Parse"));
}

#[test]
fn csharp_renders_oversized_hex_blob_as_byte_array() {
    // PUSHDATA1 of 20 non-printable bytes (0xA0..0xB3) + RET. The lift renders
    // a non-printable blob as `0x<HEX>`; a >16-digit hex value is C# error
    // CS1021 as an integer, so it must become a byte[] literal.
    let blob: Vec<u8> = (0..20).map(|i| 0xA0 + i).collect();
    let mut script = vec![0x0C, 20];
    script.extend_from_slice(&blob);
    script.push(0x40);
    let nef_bytes = build_nef(&script);
    let csharp = Decompiler::new()
        .with_inline_single_use_temps(true)
        .with_trace_comments(false)
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds")
        .csharp
        .expect("csharp output");
    assert!(
        csharp.contains("new byte[] { 0xA0, 0xA1,") && csharp.contains("0xB3 }"),
        "wide hex blob must render as a byte[] literal: {csharp}"
    );

    // Short hex (syscall hashes, CALLT indices, labels) must NOT be touched.
    assert!(!legacy_statement_to_csharp("let t0 = syscall(0xEFBEADDE);").contains("byte[]"));
    assert!(!legacy_statement_to_csharp("let t0 = callt(0x0000);").contains("byte[]"));
    assert!(!legacy_statement_to_csharp("goto label_0x0010;").contains("byte[]"));
}

#[test]
fn csharp_renders_map_literal_as_collection_initializer() {
    // PUSH4 PUSH3 PUSH2 PUSH1 PUSH2(count) PACKMAP RET keeps source order as
    // Map(1: 2, 3: 4). The
    // `Map(k: v)` form's `:` is invalid in a C# call (CS1026); it must render
    // as a collection initializer.
    let script = [0x14, 0x13, 0x12, 0x11, 0x12, 0xBE, 0x40];
    let nef_bytes = build_nef(&script);
    let csharp = Decompiler::new()
        .with_inline_single_use_temps(true)
        .with_trace_comments(false)
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds")
        .csharp
        .expect("csharp output");
    assert!(
        csharp.contains("new Map<object, object> { [1] = 2, [3] = 4 }"),
        "non-empty map literal must render as a C# collection initializer: {csharp}"
    );
    assert!(
        !csharp.contains("Map(1: 2"),
        "the invalid `Map(k: v)` form must not appear: {csharp}"
    );

    // The empty map keeps the constructor form, and the statement rewriter
    // leaves a non-map `Map(...)` (none exists in the lift) untouched.
    assert_eq!(
        legacy_statement_to_csharp("let t0 = Map();"),
        "var t0 = new Map<object, object>();"
    );
}

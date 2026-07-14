use super::super::*;
use super::*;

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

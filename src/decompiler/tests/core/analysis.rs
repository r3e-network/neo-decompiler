use super::*;

use crate::decompiler::analysis::call_graph::CallTarget;
use crate::decompiler::analysis::types::ValueType;

#[test]
fn decompilation_includes_call_graph_syscalls() {
    // Script: SYSCALL System.Runtime.GetTime, RET
    let script = [
        0x41, 0xB7, 0xC3, 0x88, 0x03, // SYSCALL 0x0388C3B7
        0x40, // RET
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    assert_eq!(decompilation.call_graph.edges.len(), 1);
    let edge = &decompilation.call_graph.edges[0];
    assert_eq!(edge.opcode, "SYSCALL");
    match &edge.target {
        CallTarget::Syscall {
            hash,
            name,
            returns_value,
        } => {
            assert_eq!(*hash, 0x0388C3B7);
            assert_eq!(
                name.as_deref(),
                Some("System.Runtime.GetTime"),
                "expected syscall name to resolve"
            );
            assert!(*returns_value);
        }
        other => panic!("unexpected call target: {other:?}"),
    }
}

#[test]
fn decompilation_includes_call_graph_internal_calls() {
    // Script layout:
    // 0x0000: CALL +4 (target=0x0004)
    // 0x0002: RET
    // 0x0003: NOP
    // 0x0004: RET
    let script = [0x34, 0x04, 0x40, 0x21, 0x40];
    let nef_bytes = build_nef(&script);
    let manifest = ContractManifest::from_json_str(
        r#"
        {
            "name": "ExampleContract",
            "supportedstandards": [],
            "features": {"storage": false, "payable": false},
            "abi": {
                "methods": [
                    {
                        "name": "main",
                        "parameters": [],
                        "returntype": "Void",
                        "offset": 0,
                        "safe": false
                    },
                    {
                        "name": "helper",
                        "parameters": [],
                        "returntype": "Void",
                        "offset": 4,
                        "safe": false
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
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    assert_eq!(decompilation.call_graph.methods.len(), 2);
    assert_eq!(decompilation.call_graph.methods[0].offset, 0);
    assert_eq!(decompilation.call_graph.methods[0].name, "main");
    assert_eq!(decompilation.call_graph.methods[1].offset, 4);
    assert_eq!(decompilation.call_graph.methods[1].name, "helper");

    assert_eq!(decompilation.call_graph.edges.len(), 1);
    let edge = &decompilation.call_graph.edges[0];
    assert_eq!(edge.opcode, "CALL");
    assert_eq!(edge.call_offset, 0);
    assert_eq!(edge.caller.offset, 0);
    assert_eq!(edge.caller.name, "main");
    match &edge.target {
        CallTarget::Internal { method } => {
            assert_eq!(method.offset, 4);
            assert_eq!(method.name, "helper");
        }
        other => panic!("unexpected call target: {other:?}"),
    }
}

#[test]
fn call_graph_resolves_relative_call_from_opcode_offset() {
    // Neo VM relative call offsets are resolved from opcode position.
    // 0x0000: CALL +2 (target=0x0002)
    // 0x0002: RET
    let script = [0x34, 0x02, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    assert_eq!(decompilation.call_graph.edges.len(), 1);
    let edge = &decompilation.call_graph.edges[0];
    assert_eq!(edge.opcode, "CALL");
    assert_eq!(edge.call_offset, 0);
    match &edge.target {
        CallTarget::Internal { method } => {
            assert_eq!(method.offset, 2);
            assert_eq!(method.name, "sub_0x0002");
        }
        other => panic!("unexpected call target: {other:?}"),
    }
}

#[test]
fn decompilation_includes_call_graph_method_tokens() {
    // Script: CALLT 0, RET.
    let script = [0x37, 0x00, 0x00, 0x40];
    let hash: [u8; 20] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
        0x0F, 0x10, 0x11, 0x12, 0x13,
    ];
    let nef_bytes = build_nef_with_single_token(&script, hash, "transfer", 2, true, 0x0F);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    assert_eq!(decompilation.call_graph.edges.len(), 1);
    let edge = &decompilation.call_graph.edges[0];
    assert_eq!(edge.opcode, "CALLT");
    match &edge.target {
        CallTarget::MethodToken {
            index,
            hash_le,
            hash_be,
            method,
            parameters_count,
            has_return_value,
            call_flags,
        } => {
            assert_eq!(*index, 0);
            assert_eq!(method, "transfer");
            assert_eq!(*parameters_count, 2);
            assert!(*has_return_value);
            assert_eq!(*call_flags, 0x0F);

            let expected_le = hex::encode_upper(hash);
            let mut reversed = hash;
            reversed.reverse();
            let expected_be = hex::encode_upper(reversed);
            assert_eq!(hash_le, &expected_le);
            assert_eq!(hash_be, &expected_be);
        }
        other => panic!("unexpected call target: {other:?}"),
    }
}

#[test]
fn decompilation_includes_indirect_calls() {
    // Script:
    // CALLA          (no operand — pops Pointer from stack)
    // CALLT 0x0001   (U16 token index)
    // RET
    let script = [0x36, 0x37, 0x01, 0x00, 0x40];
    let hash = [0x42u8; 20];
    let nef_bytes = build_nef_with_single_token(&script, hash, "stub", 0, false, 0x0F);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    assert_eq!(decompilation.call_graph.edges.len(), 2);
    assert_eq!(decompilation.call_graph.edges[0].opcode, "CALLA");
    match &decompilation.call_graph.edges[0].target {
        CallTarget::Indirect { opcode, operand } => {
            assert_eq!(opcode, "CALLA");
            assert_eq!(*operand, None);
        }
        other => panic!("unexpected call target: {other:?}"),
    }

    assert_eq!(decompilation.call_graph.edges[1].opcode, "CALLT");
    match &decompilation.call_graph.edges[1].target {
        CallTarget::Indirect { opcode, operand } => {
            assert_eq!(opcode, "CALLT");
            assert_eq!(*operand, Some(1));
        }
        other => panic!("unexpected call target: {other:?}"),
    }
}

#[test]
fn decompilation_resolves_pusha_calla_to_internal_call_edge() {
    // Script layout:
    // 0x0000: PUSHA +10 (target = 0x000A)
    // 0x0005: CALLA
    // 0x0006: RET
    // 0x0007..0x0009: NOP padding
    // 0x000A: INITSLOT 0,0
    // 0x000D: RET
    let script = [
        0x0A, 0x0A, 0x00, 0x00, 0x00, // PUSHA +10
        0x36, // CALLA
        0x40, // RET
        0x21, 0x21, 0x21, // NOP x3
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
    ];

    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let edge = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "CALLA")
        .expect("CALLA edge present");

    match &edge.target {
        CallTarget::Internal { method } => {
            assert_eq!(method.offset, 0x000A);
            assert_eq!(method.name, "sub_0x000A");
        }
        other => panic!("expected resolved internal CALLA target, got: {other:?}"),
    }
}

#[test]
fn decompilation_resolves_local_pointer_flow_into_calla_edge() {
    // Script layout:
    // 0x0000: PUSHA +9  (target = 0x0009)
    // 0x0005: STLOC0
    // 0x0006: LDLOC0
    // 0x0007: CALLA
    // 0x0008: RET
    // 0x0009: INITSLOT 0,0
    // 0x000C: RET
    let script = [
        0x0A, 0x09, 0x00, 0x00, 0x00, // PUSHA +9
        0x70, // STLOC0
        0x68, // LDLOC0
        0x36, // CALLA
        0x40, // RET
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
    ];

    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let edge = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "CALLA")
        .expect("CALLA edge present");

    match &edge.target {
        CallTarget::Internal { method } => {
            assert_eq!(method.offset, 0x0009);
            assert_eq!(method.name, "sub_0x0009");
        }
        other => panic!("expected resolved local-flow CALLA target, got: {other:?}"),
    }
}

#[test]
fn decompilation_resolves_local_pointer_flow_with_nop_before_calla() {
    // Script layout:
    // 0x0000: PUSHA +10 (target = 0x000A)
    // 0x0005: STLOC0
    // 0x0006: LDLOC0
    // 0x0007: NOP
    // 0x0008: CALLA
    // 0x0009: RET
    // 0x000A: INITSLOT 0,0
    // 0x000D: RET
    let script = [
        0x0A, 0x0A, 0x00, 0x00, 0x00, // PUSHA +10
        0x70, // STLOC0
        0x68, // LDLOC0
        0x21, // NOP
        0x36, // CALLA
        0x40, // RET
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
    ];

    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let edge = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "CALLA")
        .expect("CALLA edge present");

    match &edge.target {
        CallTarget::Internal { method } => {
            assert_eq!(method.offset, 0x000A);
            assert_eq!(method.name, "sub_0x000A");
        }
        other => panic!("expected resolved local-flow CALLA target, got: {other:?}"),
    }
}

#[test]
fn decompilation_resolves_multi_hop_local_pointer_flow_into_calla_edge() {
    // Script layout:
    // 0x0000: PUSHA +12 (target = 0x000C)
    // 0x0005: STLOC0
    // 0x0006: LDLOC0
    // 0x0007: STLOC1
    // 0x0008: LDLOC1
    // 0x0009: CALLA
    // 0x000A: RET
    // 0x000B: NOP
    // 0x000C: INITSLOT 0,0
    // 0x000F: RET
    let script = [
        0x0A, 0x0C, 0x00, 0x00, 0x00, // PUSHA +12
        0x70, // STLOC0
        0x68, // LDLOC0
        0x71, // STLOC1
        0x69, // LDLOC1
        0x36, // CALLA
        0x40, // RET
        0x21, // NOP
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
    ];

    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let edge = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "CALLA")
        .expect("CALLA edge present");

    match &edge.target {
        CallTarget::Internal { method } => {
            assert_eq!(method.offset, 0x000C);
            assert_eq!(method.name, "sub_0x000C");
        }
        other => panic!("expected resolved multi-hop local CALLA target, got: {other:?}"),
    }
}

#[test]
fn decompilation_does_not_resolve_local_pointer_across_method_boundary() {
    // Script layout:
    // 0x0000: INITSLOT 1,0
    // 0x0003: PUSHA +14 (target = 0x0011)
    // 0x0008: STLOC0
    // 0x0009: RET
    // 0x000A: INITSLOT 1,0
    // 0x000D: LDLOC0
    // 0x000E: CALLA
    // 0x000F: RET
    // 0x0010: NOP
    // 0x0011: INITSLOT 0,0
    // 0x0014: RET
    let script = [
        0x57, 0x01, 0x00, // INITSLOT 1,0
        0x0A, 0x0E, 0x00, 0x00, 0x00, // PUSHA +14
        0x70, // STLOC0
        0x40, // RET
        0x57, 0x01, 0x00, // INITSLOT 1,0
        0x68, // LDLOC0
        0x36, // CALLA
        0x40, // RET
        0x21, // NOP
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
    ];

    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let edge = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "CALLA")
        .expect("CALLA edge present");

    match &edge.target {
        CallTarget::Indirect { opcode, operand } => {
            assert_eq!(opcode, "CALLA");
            assert_eq!(*operand, None);
        }
        other => panic!("expected cross-method local CALLA to remain indirect, got: {other:?}"),
    }
}

#[test]
fn type_inference_uses_manifest_parameter_types_for_offsetless_entry_method() {
    // Script: LDARG0; STARG0; RET
    // No INITSLOT prologue, so the entry shape must come from the manifest.
    let script = [0x78, 0x80, 0x40];
    let nef_bytes = build_nef(&script);
    let manifest = ContractManifest::from_json_str(
        r#"
        {
            "name": "OffsetlessTypes",
            "supportedstandards": [],
            "features": {"storage": false, "payable": false},
            "abi": {
                "methods": [
                    {
                        "name": "main",
                        "parameters": [
                            {"name": "owner", "type": "Hash160"}
                        ],
                        "returntype": "Void",
                        "safe": false
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
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let method = &decompilation.types.methods[0];
    assert_eq!(method.method.name, "main");
    assert_eq!(method.arguments.len(), 1);
    assert_eq!(method.arguments[0], ValueType::ByteString);
}

#[test]
fn type_inference_tracks_read_only_fixed_argument_slots_without_initslot() {
    // Script: LDARG0; RET
    let script = [0x78, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let method = &decompilation.types.methods[0];
    assert_eq!(method.arguments.len(), 1);
    assert_eq!(method.arguments[0], ValueType::Unknown);
}

#[test]
fn call_graph_attributes_helper_syscall_to_inferred_helper_method() {
    // Script layout:
    // 0x0000: CALL +4 (target=0x0004)
    // 0x0002: RET
    // 0x0003: NOP
    // 0x0004: SYSCALL System.Runtime.GetTime
    // 0x0009: RET
    let script = [
        0x34, 0x04, // CALL +4
        0x40, // RET
        0x21, // NOP
        0x41, 0xB7, 0xC3, 0x88, 0x03, // SYSCALL 0x0388C3B7
        0x40, // RET
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    assert_eq!(decompilation.call_graph.edges.len(), 2);
    let helper_edge = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "SYSCALL")
        .expect("helper syscall edge");
    assert_eq!(helper_edge.call_offset, 4);
    assert_eq!(helper_edge.caller.offset, 4);
    assert_eq!(helper_edge.caller.name, "sub_0x0004");
}

#[test]
fn call_graph_attributes_pusha_calla_helper_syscall_to_inferred_helper_method() {
    // Script layout:
    // 0x0000: PUSHA +8 (target=0x0008)
    // 0x0005: CALLA
    // 0x0006: RET
    // 0x0007: NOP
    // 0x0008: SYSCALL System.Runtime.GetTime
    // 0x000D: RET
    let script = [
        0x0A, 0x08, 0x00, 0x00, 0x00, // PUSHA +8
        0x36, // CALLA
        0x40, // RET
        0x21, // NOP
        0x41, 0xB7, 0xC3, 0x88, 0x03, // SYSCALL 0x0388C3B7
        0x40, // RET
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let helper_edge = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "SYSCALL")
        .expect("helper syscall edge");
    assert_eq!(helper_edge.call_offset, 8);
    assert_eq!(helper_edge.caller.offset, 8);
    assert_eq!(helper_edge.caller.name, "sub_0x0008");
}

#[test]
fn call_graph_attributes_ldarg_calla_helper_syscall_to_inferred_helper_method() {
    // Script layout:
    // 0x0000: PUSHA +15 (target=0x000F)
    // 0x0005: CALL +4 (target=0x0009)
    // 0x0007: RET
    // 0x0008: NOP
    // 0x0009: INITSLOT 0,1
    // 0x000C: LDARG0
    // 0x000D: CALLA
    // 0x000E: RET
    // 0x000F: SYSCALL System.Runtime.GetTime
    // 0x0014: RET
    let script = [
        0x0A, 0x0F, 0x00, 0x00, 0x00, // PUSHA +15
        0x34, 0x04, // CALL +4
        0x40, // RET
        0x21, // NOP
        0x57, 0x00, 0x01, // INITSLOT 0,1
        0x78, // LDARG0
        0x36, // CALLA
        0x40, // RET
        0x41, 0xB7, 0xC3, 0x88, 0x03, // SYSCALL 0x0388C3B7
        0x40, // RET
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let helper_edge = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "SYSCALL")
        .expect("helper syscall edge");
    assert_eq!(helper_edge.call_offset, 0x000F);
    assert_eq!(helper_edge.caller.offset, 0x000F);
    assert_eq!(helper_edge.caller.name, "sub_0x000F");
}

#[test]
fn call_graph_attributes_ldloc_from_argument_calla_helper_syscall_to_inferred_helper_method() {
    // Script layout:
    // 0x0000: PUSHA +17 (target=0x0011)
    // 0x0005: CALL +4 (target=0x0009)
    // 0x0007: RET
    // 0x0008: NOP
    // 0x0009: INITSLOT 1,1
    // 0x000C: LDARG0
    // 0x000D: STLOC0
    // 0x000E: LDLOC0
    // 0x000F: CALLA
    // 0x0010: RET
    // 0x0011: SYSCALL System.Runtime.GetTime
    // 0x0016: RET
    let script = [
        0x0A, 0x11, 0x00, 0x00, 0x00, // PUSHA +17
        0x34, 0x04, // CALL +4
        0x40, // RET
        0x21, // NOP
        0x57, 0x01, 0x01, // INITSLOT 1,1
        0x78, // LDARG0
        0x70, // STLOC0
        0x68, // LDLOC0
        0x36, // CALLA
        0x40, // RET
        0x41, 0xB7, 0xC3, 0x88, 0x03, // SYSCALL 0x0388C3B7
        0x40, // RET
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let helper_edge = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "SYSCALL")
        .expect("helper syscall edge");
    assert_eq!(helper_edge.call_offset, 0x0011);
    assert_eq!(helper_edge.caller.offset, 0x0011);
    assert_eq!(helper_edge.caller.name, "sub_0x0011");
}

#[test]
fn call_graph_resolves_nested_pusha_argument_through_calla_helper() {
    // Script layout:
    // 0x0000: PUSHA +18 (target = 0x0012)  // helper2 pointer argument
    // 0x0005: PUSHA +7  (target = 0x000C)  // helper1 callee
    // 0x000A: CALLA
    // 0x000B: RET
    // 0x000C: INITSLOT 0,1
    // 0x000F: LDARG0
    // 0x0010: CALLA
    // 0x0011: RET
    // 0x0012: SYSCALL System.Runtime.GetTime
    // 0x0017: RET
    let script = [
        0x0A, 0x12, 0x00, 0x00, 0x00, // PUSHA +18
        0x0A, 0x07, 0x00, 0x00, 0x00, // PUSHA +7
        0x36, // CALLA
        0x40, // RET
        0x57, 0x00, 0x01, // INITSLOT 0,1
        0x78, // LDARG0
        0x36, // CALLA
        0x40, // RET
        0x41, 0xB7, 0xC3, 0x88, 0x03, // SYSCALL 0x0388C3B7
        0x40, // RET
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let nested_calla = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "CALLA" && edge.call_offset == 0x0010)
        .expect("nested CALLA edge present");
    match &nested_calla.target {
        CallTarget::Internal { method } => {
            assert_eq!(method.offset, 0x0012);
            assert_eq!(method.name, "sub_0x0012");
        }
        other => panic!("expected nested CALLA to resolve helper target, got: {other:?}"),
    }

    let helper_edge = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "SYSCALL")
        .expect("helper syscall edge");
    assert_eq!(helper_edge.call_offset, 0x0012);
    assert_eq!(helper_edge.caller.offset, 0x0012);
    assert_eq!(helper_edge.caller.name, "sub_0x0012");
}

#[test]
fn call_graph_resolves_nested_pusha_argument_through_calla_helper_without_initslot() {
    // Script layout:
    // 0x0000: PUSHA +11 (target = 0x000B) // helper2 pointer argument
    // 0x0005: CALL +3  (target = 0x0008) // helper1 callee, no INITSLOT
    // 0x0007: RET
    // 0x0008: LDARG0
    // 0x0009: CALLA
    // 0x000A: RET
    // 0x000B: SYSCALL System.Runtime.GetTime
    // 0x0010: RET
    let script = [
        0x0A, 0x0B, 0x00, 0x00, 0x00, // PUSHA +11
        0x34, 0x03, // CALL +3
        0x40, // RET
        0x78, // LDARG0
        0x36, // CALLA
        0x40, // RET
        0x41, 0xB7, 0xC3, 0x88, 0x03, // SYSCALL 0x0388C3B7
        0x40, // RET
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let nested_calla = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "CALLA" && edge.call_offset == 0x0009)
        .expect("nested CALLA edge present");
    match &nested_calla.target {
        CallTarget::Internal { method } => {
            assert_eq!(method.offset, 0x000B);
            assert_eq!(method.name, "sub_0x000B");
        }
        other => panic!("expected CALLA without INITSLOT helper to resolve target, got: {other:?}"),
    }
}

#[test]
fn call_graph_resolves_two_level_nested_calla_argument_chain() {
    // Script layout:
    // 0x0000: PUSHA +20 (target = 0x0014) // helper2 pointer
    // 0x0005: PUSHA +21 (target = 0x001A) // helper3 pointer
    // 0x000A: CALL +3  (target = 0x000D) // helper1
    // 0x000C: RET
    // 0x000D: INITSLOT 0,2
    // 0x0010: LDARG0   // helper3
    // 0x0011: LDARG1   // helper2
    // 0x0012: CALLA    // helper2(helper3)
    // 0x0013: RET
    // 0x0014: INITSLOT 0,1
    // 0x0017: LDARG0   // helper3
    // 0x0018: CALLA    // helper3()
    // 0x0019: RET
    // 0x001A: SYSCALL System.Runtime.GetTime
    // 0x001F: RET
    let script = [
        0x0A, 0x14, 0x00, 0x00, 0x00, // PUSHA +20
        0x0A, 0x15, 0x00, 0x00, 0x00, // PUSHA +21
        0x34, 0x03, // CALL +3
        0x40, // RET
        0x57, 0x00, 0x02, // INITSLOT 0,2
        0x78, // LDARG0
        0x79, // LDARG1
        0x36, // CALLA
        0x40, // RET
        0x57, 0x00, 0x01, // INITSLOT 0,1
        0x78, // LDARG0
        0x36, // CALLA
        0x40, // RET
        0x41, 0xB7, 0xC3, 0x88, 0x03, // SYSCALL 0x0388C3B7
        0x40, // RET
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let nested_calla = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "CALLA" && edge.call_offset == 0x0018)
        .expect("second-level CALLA edge present");
    match &nested_calla.target {
        CallTarget::Internal { method } => {
            assert_eq!(method.offset, 0x001A);
            assert_eq!(method.name, "sub_0x001A");
        }
        other => panic!("expected second-level CALLA to resolve helper target, got: {other:?}"),
    }

    let helper_edge = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "SYSCALL")
        .expect("helper syscall edge");
    assert_eq!(helper_edge.call_offset, 0x001A);
    assert_eq!(helper_edge.caller.offset, 0x001A);
    assert_eq!(helper_edge.caller.name, "sub_0x001A");
}

#[test]
fn inferred_method_starts_tolerate_malformed_tryl_operand() {
    use crate::instruction::{Instruction, OpCode, Operand};

    let instructions = vec![
        Instruction::new(
            0,
            OpCode::TryL,
            Some(Operand::Bytes(vec![0x01, 0x02, 0x03])),
        ),
        Instruction::new(1, OpCode::Ret, None),
    ];

    let inferred = crate::decompiler::helpers::inferred_method_starts(&instructions, None);
    assert_eq!(inferred, vec![0]);
}

#[test]
fn decompilation_resolves_pickitem_delegate_array_into_calla_edge() {
    // Script layout:
    // 0x0000: NEWARRAY0
    // 0x0001: STLOC0
    // 0x0002: LDLOC0
    // 0x0003: PUSHA +11 (target = 0x000E)
    // 0x0008: APPEND
    // 0x0009: LDLOC0
    // 0x000A: PUSH0
    // 0x000B: PICKITEM
    // 0x000C: CALLA
    // 0x000D: RET
    // 0x000E: INITSLOT 0,0
    // 0x0011: RET
    let script = [
        0xC2, // NEWARRAY0
        0x70, // STLOC0
        0x68, // LDLOC0
        0x0A, 0x0B, 0x00, 0x00, 0x00, // PUSHA +11
        0xCF, // APPEND
        0x68, // LDLOC0
        0x10, // PUSH0
        0xCE, // PICKITEM
        0x36, // CALLA
        0x40, // RET
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
    ];

    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let edge = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "CALLA")
        .expect("CALLA edge present");

    match &edge.target {
        CallTarget::Internal { method } => {
            assert_eq!(method.offset, 0x000E);
            assert_eq!(method.name, "sub_0x000E");
        }
        other => {
            panic!("expected PICKITEM delegate CALLA to resolve helper target, got: {other:?}")
        }
    }
}

#[test]
fn decompilation_resolves_pickitem_delegate_array_through_local_alias() {
    // Script layout:
    // NEWARRAY0; STLOC0
    // LDLOC0; DUP; STLOC1
    // PUSHA +14 (target=0x0016); APPEND
    // LDLOC1; PUSH0; PICKITEM; STLOC2
    // LDLOC2; CALLA; RET
    // target: INITSLOT 0,0; RET
    let script = [
        0xC2, // NEWARRAY0
        0x70, // STLOC0
        0x68, // LDLOC0
        0x4A, // DUP
        0x71, // STLOC1
        0x0A, 0x11, 0x00, 0x00, 0x00, // PUSHA +17
        0xCF, // APPEND
        0x69, // LDLOC1
        0x10, // PUSH0
        0xCE, // PICKITEM
        0x72, // STLOC2
        0x6A, // LDLOC2
        0x36, // CALLA
        0x40, // RET
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
    ];

    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let edge = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "CALLA")
        .expect("CALLA edge present");

    match &edge.target {
        CallTarget::Internal { method } => {
            assert_eq!(method.offset, 0x0016);
            assert_eq!(method.name, "sub_0x0016");
        }
        other => {
            panic!("expected aliased delegate-array CALLA to resolve helper target, got: {other:?}")
        }
    }
}

#[test]
fn decompilation_resolves_duplicated_pointer_into_calla_edge() {
    // Script layout:
    // 0x0000: PUSHA +8 (target = 0x0008)
    // 0x0005: DUP
    // 0x0006: CALLA
    // 0x0007: RET
    // 0x0008: INITSLOT 0,0
    // 0x000B: RET
    let script = [
        0x0A, 0x08, 0x00, 0x00, 0x00, // PUSHA +8
        0x4A, // DUP
        0x36, // CALLA
        0x40, // RET
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
    ];

    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let edge = decompilation
        .call_graph
        .edges
        .iter()
        .find(|edge| edge.opcode == "CALLA")
        .expect("CALLA edge present");

    match &edge.target {
        CallTarget::Internal { method } => {
            assert_eq!(method.offset, 0x0008);
            assert_eq!(method.name, "sub_0x0008");
        }
        other => panic!("expected DUP-fed CALLA target to resolve, got: {other:?}"),
    }
}

#[test]
fn decompilation_includes_slot_xrefs() {
    // Script:
    // INITSLOT 1 local, 0 args
    // PUSH1; STLOC0
    // LDLOC0; RET
    let script = [0x57, 0x01, 0x00, 0x11, 0x70, 0x68, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let method = &decompilation.xrefs.methods[0];
    assert_eq!(method.locals.len(), 1);
    assert!(method.locals[0].writes.contains(&4));
    assert!(method.locals[0].reads.contains(&5));
}

#[test]
fn decompilation_includes_argument_slot_xrefs() {
    // Script:
    // INITSLOT 0 locals, 2 args
    // LDARG0
    // PUSH1; STARG1
    // RET
    let script = [0x57, 0x00, 0x02, 0x78, 0x11, 0x81, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let method = &decompilation.xrefs.methods[0];
    assert_eq!(method.arguments.len(), 2);
    assert!(method.arguments[0].reads.contains(&3));
    assert!(method.arguments[1].writes.contains(&5));
}

#[test]
fn decompilation_includes_indexed_slot_xrefs() {
    // Script:
    // INITSLOT 0 locals, 8 args
    // LDARG 7
    // PUSH1; STARG 7
    // RET
    let script = [
        0x57, 0x00, 0x08, // INITSLOT
        0x7F, 0x07, // LDARG 7
        0x11, // PUSH1
        0x87, 0x07, // STARG 7
        0x40, // RET
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let method = &decompilation.xrefs.methods[0];
    assert!(method.arguments.len() >= 8);
    assert!(method.arguments[7].reads.contains(&3));
    assert!(method.arguments[7].writes.contains(&6));
}

#[test]
fn decompilation_includes_static_slot_xrefs() {
    // Script:
    // INITSSLOT 2
    // PUSH1; STSFLD0
    // LDSFLD0; RET
    let script = [0x56, 0x02, 0x11, 0x60, 0x58, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let method = &decompilation.xrefs.methods[0];
    assert_eq!(method.statics.len(), 2);
    assert!(method.statics[0].writes.contains(&3));
    assert!(method.statics[0].reads.contains(&4));
}

#[test]
fn decompilation_includes_indexed_static_slot_xrefs() {
    // Script:
    // INITSSLOT 8
    // PUSH1; STSFLD 7
    // LDSFLD 7; RET
    let script = [
        0x56, 0x08, // INITSSLOT 8
        0x11, // PUSH1
        0x67, 0x07, // STSFLD 7
        0x5F, 0x07, // LDSFLD 7
        0x40, // RET
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let method = &decompilation.xrefs.methods[0];
    assert!(method.statics.len() >= 8);
    assert!(method.statics[7].writes.contains(&3));
    assert!(method.statics[7].reads.contains(&5));
}

#[test]
fn decompilation_infers_collection_types_for_locals() {
    // Script:
    // INITSLOT 1 local, 0 args
    // NEWMAP; STLOC0
    // LDLOC0; RET
    let script = [0x57, 0x01, 0x00, 0xC8, 0x70, 0x68, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let method = &decompilation.types.methods[0];
    assert_eq!(method.locals.len(), 1);
    assert_eq!(method.locals[0], ValueType::Map);
}

#[test]
fn decompilation_propagates_manifest_argument_types() {
    // Script:
    // INITSLOT 0 locals, 2 args
    // RET
    let script = [0x57, 0x00, 0x02, 0x40];
    let nef_bytes = build_nef(&script);
    let manifest = ContractManifest::from_json_str(
        r#"
        {
            "name": "ExampleContract",
            "supportedstandards": [],
            "features": {"storage": false, "payable": false},
            "abi": {
                "methods": [
                    {
                        "name": "main",
                        "parameters": [
                            {"name": "amount", "type": "Integer"},
                            {"name": "flag", "type": "Boolean"}
                        ],
                        "returntype": "Void",
                        "offset": 0,
                        "safe": false
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
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let method = &decompilation.types.methods[0];
    assert_eq!(
        method.arguments,
        vec![ValueType::Integer, ValueType::Boolean]
    );
}

#[test]
fn decompilation_infers_static_slot_types() {
    // Script:
    // INITSSLOT 1
    // NEWMAP; STSFLD0
    // RET
    let script = [0x56, 0x01, 0xC8, 0x60, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    assert_eq!(decompilation.types.statics, vec![ValueType::Map]);
}

#[test]
fn decompilation_infers_packmap_types() {
    // Script:
    // INITSLOT 1 local, 0 args
    // PUSH0; PACKMAP; STLOC0
    // RET
    let script = [0x57, 0x01, 0x00, 0x10, 0xBE, 0x70, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let method = &decompilation.types.methods[0];
    assert_eq!(method.locals, vec![ValueType::Map]);
}

#[test]
fn decompilation_infers_convert_target_types() {
    // Script:
    // INITSLOT 1 local, 0 args
    // PUSHDATA1 0xAA; CONVERT ByteString; STLOC0
    // RET
    let script = [
        0x57, 0x01, 0x00, // INITSLOT
        0x0C, 0x01, 0xAA, // PUSHDATA1
        0xDB, 0x28, // CONVERT ByteString
        0x70, // STLOC0
        0x40, // RET
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::Pseudocode)
        .expect("decompile succeeds");

    let method = &decompilation.types.methods[0];
    assert_eq!(method.locals, vec![ValueType::ByteString]);
}

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
    // 0x0000: CALL +2 (target=0x0004)
    // 0x0002: RET
    // 0x0003: NOP
    // 0x0004: RET
    let script = [0x34, 0x02, 0x40, 0x21, 0x40];
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

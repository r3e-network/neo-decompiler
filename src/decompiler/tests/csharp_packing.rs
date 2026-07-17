use super::super::*;
use super::*;

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
        pack.source.contains("new BigInteger[] { 1, 2 }"),
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
            "__NeoDecompilerConvertInteger(1)",
        ),
        (
            "IsTypeTag",
            crate::instruction::OpCode::Istype,
            "__NeoDecompilerIsTypeInteger(1)",
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
                rendered.source.contains("return new BigInteger[(int)(1)];"),
                "NEWARRAY_T renders a compile-oriented typed array expression:\n{}",
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
        rendered
            .source
            .contains("__NeoDecompilerConvertInteger_1(1)"),
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
        rendered
            .source
            .contains("__NeoDecompilerUnpackPackStruct_1(1)"),
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
        rendered.source.contains("__NeoDecompilerBareThrow_1();"),
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

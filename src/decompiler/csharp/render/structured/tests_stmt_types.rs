use super::super::*;

#[test]
fn typed_statement_references_use_planned_csharp_identifiers() {
    let body = Block::from(vec![
        Stmt::assign("class", Expr::int(1)),
        Stmt::ret(Expr::var("class")),
    ]);
    let symbols = BTreeMap::from([(
        "class".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Local(0),
            value_type: ValueType::Integer,
        },
    )]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Value, false),
        "BigInteger @class = 1;\nreturn @class;"
    );
}

#[test]
fn typed_boundaries_render_valid_explicit_conversions() {
    let body = Block::from(vec![
        Stmt::assign("text", Expr::var("buffer")),
        Stmt::ret(Expr::var("buffer")),
    ]);
    let symbols = BTreeMap::from([
        (
            "text".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::ByteString,
            },
        ),
        (
            "buffer".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Buffer,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(
        super::super::super::stmt::render_block_with_trace(
            &body,
            &plan,
            &symbols,
            ReturnBehavior::Value,
            false,
            "__assert",
            "__NeoDecompilerVmException",
            None,
            None,
            &BTreeMap::new(),
            &BTreeMap::new(),
            Some("UInt160"),
            None,
            &[],
            &BTreeMap::new(),
        ),
        "ByteString text = (ByteString)(buffer);\nreturn (UInt160)(buffer);"
    );
}

#[test]
fn runtime_typed_literal_indexes_keep_object_array_cast_boundaries() {
    let body = Block::from(vec![
        Stmt::assign(
            "items",
            Expr::Array(vec![
                Expr::int(7),
                Expr::Literal(Literal::String("ready".to_string())),
            ]),
        ),
        Stmt::ret(Expr::index(Expr::var("items"), Expr::int(0))),
    ]);
    let symbols = BTreeMap::from([(
        "items".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Array,
        },
    )]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(
        super::super::super::stmt::render_block_with_trace(
            &body,
            &plan,
            &symbols,
            ReturnBehavior::Value,
            false,
            "__assert",
            "__NeoDecompilerVmException",
            None,
            None,
            &BTreeMap::new(),
            &BTreeMap::new(),
            Some("BigInteger"),
            None,
            &[],
            &BTreeMap::new(),
        ),
        "object[] items = new object[] { 7, \"ready\" };\nreturn (BigInteger)(dynamic)(items[0]);"
    );
}

#[test]
fn typed_internal_call_boundaries_use_exact_resolved_return_types() {
    let call = Expr::call(
        SemanticCallTarget::Internal {
            offset: 42,
            name: "helper".to_string(),
        },
        Vec::new(),
    );
    let body = Block::from(vec![Stmt::ret(call)]);
    let symbols = BTreeMap::new();
    let plan = plan_declarations(&body, &symbols, true);

    let render = |return_types: &BTreeMap<usize, String>| {
        super::super::super::stmt::render_block_with_trace(
            &body,
            &plan,
            &symbols,
            ReturnBehavior::Value,
            false,
            "__assert",
            "__NeoDecompilerVmException",
            None,
            None,
            &BTreeMap::new(),
            return_types,
            Some("BigInteger"),
            None,
            &[],
            &BTreeMap::new(),
        )
    };

    assert_eq!(
        render(&BTreeMap::from([(42, "BigInteger".to_string())])),
        "return helper();"
    );
    assert_eq!(
        render(&BTreeMap::from([(42, "ByteString".to_string())])),
        "return (BigInteger)(helper());"
    );
    assert_eq!(
        render(&BTreeMap::new()),
        "return (BigInteger)(dynamic)(helper());"
    );
}

#[test]
fn typed_ambient_assignments_render_boundary_conversions() {
    let body = Block::from(vec![Stmt::assign("static0", Expr::Unknown)]);
    let symbols = BTreeMap::from([(
        "static0".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Static(0),
            value_type: ValueType::Integer,
        },
    )]);

    let typed = plan_declarations(&body, &symbols, true);
    assert_eq!(
        render_block(&body, &typed, &symbols, ReturnBehavior::Void, false),
        "static0 = (BigInteger)(dynamic)((dynamic)null);"
    );

    let dynamic = plan_declarations(&body, &symbols, false);
    assert_eq!(
        render_block(&body, &dynamic, &symbols, ReturnBehavior::Void, false),
        "static0 = (dynamic)null;"
    );
}

#[test]
fn typed_static_field_boundaries_use_contract_field_types() {
    let body = Block::from(vec![Stmt::assign("static0", Expr::Unknown)]);
    let symbols = BTreeMap::new();
    let static_field_types =
        BTreeMap::from([(String::from("static0"), String::from("BigInteger"))]);
    let plan =
        plan_declarations(&body, &symbols, true).with_static_field_types(&static_field_types);

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Void, false),
        "static0 = (BigInteger)(dynamic)((dynamic)null);"
    );
}

#[test]
fn concrete_native_reference_arrays_flow_into_object_array_storage() {
    let body = Block::from(vec![Stmt::assign(
        "static0",
        Expr::call(
            SemanticCallTarget::MethodToken {
                index: 0,
                name: "getDesignatedByRole".to_string(),
                hash_le: Some("E295E391544C178AD94F03EC4DCDFF78534ECF49".to_string()),
                call_flags: Some(0x0F),
            },
            vec![Expr::int(8), Expr::int(0)],
        ),
    )]);
    let symbols = BTreeMap::from([(
        "static0".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Static(0),
            value_type: ValueType::Array,
        },
    )]);
    let plan =
        plan_declarations(&body, &symbols, true).with_static_field_types(&BTreeMap::from([(
            "static0".to_string(),
            "object[]".to_string(),
        )]));

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Void, false),
        "static0 = RoleManagement.GetDesignatedByRole((Role)(int)(8), 0);"
    );
}

#[test]
fn known_native_void_calls_render_as_statements() {
    let body = Block::from(vec![Stmt::expr(Expr::call(
        SemanticCallTarget::MethodToken {
            index: 0,
            name: "destroy".to_string(),
            hash_le: Some("FDA3FA4346EA532A258FC497DDADDB6437C9FDFF".to_string()),
            call_flags: Some(0x0F),
        },
        vec![],
    ))]);
    let plan = plan_declarations(&body, &BTreeMap::new(), true);

    assert_eq!(
        render_block(&body, &plan, &BTreeMap::new(), ReturnBehavior::Void, false),
        "ContractManagement.Destroy();"
    );
}

#[test]
fn compiler_debug_notify_omits_the_packed_state_temp() {
    let body = Block::from(vec![
        Stmt::assign(
            "state",
            Expr::Array(vec![Expr::Literal(Literal::String("message".to_string()))]),
        ),
        Stmt::expr(Expr::call(
            SemanticCallTarget::Syscall {
                hash: 0x616F_0195,
                name: Some("System.Runtime.Notify".to_string()),
            },
            vec![
                Expr::Literal(Literal::String("Debug".to_string())),
                Expr::var("state"),
            ],
        )),
    ]);
    let symbols = BTreeMap::from([(
        "state".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Array,
        },
    )]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Void, true),
        "Runtime.Debug(\"message\");"
    );
}

#[test]
fn typed_index_definitions_ignore_stale_slot_collection_types() {
    let body = Block::from(vec![
        Stmt::assign("t0", Expr::index(Expr::var("items"), Expr::int(0))),
        Stmt::assign("loc0", Expr::var("t0")),
    ]);
    let symbols = BTreeMap::from([
        (
            "items".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Array,
            },
        ),
        (
            "t0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "loc0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Struct,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);
    let rendered = render_block(&body, &plan, &symbols, ReturnBehavior::Void, false);

    assert!(rendered.contains("dynamic t0 = "), "{rendered}");
    assert!(rendered.contains("dynamic loc0 = t0;"), "{rendered}");
    assert!(!rendered.contains("object[]"), "{rendered}");
}

#[test]
fn typed_index_copy_provenance_converges_independently_of_statement_order() {
    let body = Block::from(vec![
        Stmt::assign("loc0", Expr::var("t0")),
        Stmt::assign("t0", Expr::index(Expr::var("items"), Expr::int(0))),
    ]);
    let symbols = BTreeMap::from([
        (
            "items".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Array,
            },
        ),
        (
            "t0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "loc0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Struct,
            },
        ),
    ]);

    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(plan.declarations["t0"].csharp_type, "dynamic");
    assert_eq!(plan.declarations["loc0"].csharp_type, "dynamic");
}

#[test]
fn typed_index_assignments_dynamicize_parameter_and_static_storage() {
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "IndexStorage",
            "abi": { "methods": [{
                "name": "store",
                "parameters": [
                    { "name": "target", "type": "Array" },
                    { "name": "items", "type": "Array" }
                ],
                "returntype": "Array",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");
    let instructions = vec![
        Instruction::new(0, OpCode::Initslot, Some(Operand::Bytes(vec![0, 2]))),
        Instruction::new(3, OpCode::Push1, None),
        Instruction::new(4, OpCode::Push2, None),
        Instruction::new(5, OpCode::Push2, None),
        Instruction::new(6, OpCode::Packstruct, None),
        Instruction::new(7, OpCode::Dup, None),
        Instruction::new(8, OpCode::Push0, None),
        Instruction::new(9, OpCode::Ldarg1, None),
        Instruction::new(10, OpCode::Setitem, None),
        Instruction::new(11, OpCode::Unpack, None),
        Instruction::new(12, OpCode::Drop, None),
        Instruction::new(13, OpCode::Drop, None),
        Instruction::new(14, OpCode::Dup, None),
        Instruction::new(15, OpCode::Starg0, None),
        Instruction::new(16, OpCode::Stsfld0, None),
        Instruction::new(17, OpCode::Ldarg0, None),
        Instruction::new(18, OpCode::Ret, None),
    ];
    let types = TypeInfo {
        statics: vec![ValueType::Struct],
        ..TypeInfo::default()
    };

    let plans = build_csharp_method_plans(
        &instructions,
        Some(&manifest),
        &CallGraph::default(),
        &MethodContracts::default(),
        &types,
        &[0],
    );
    let contract = plan_contract_symbols(
        &types,
        &plans.method_symbol_maps().iter().collect::<Vec<_>>(),
        true,
        plans.index_defined_statics(),
    );

    assert_eq!(plans.manifest_method(0).parameters[0].ty, "dynamic");
    assert_eq!(contract.static_fields[0].csharp_type, "dynamic");
}

#[test]
fn hoisted_phi_declarations_are_default_initialized() {
    let body = Block::from(vec![
        Stmt::ControlFlow(Box::new(ControlFlow::if_then(
            Expr::var("condition"),
            Block::from(vec![Stmt::assign(
                "p3_0",
                Expr::Literal(Literal::Bool(true)),
            )]),
        ))),
        Stmt::ret(Expr::var("p3_0")),
    ]);
    let symbols = BTreeMap::from([
        (
            "condition".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Boolean,
            },
        ),
        (
            "p3_0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Phi,
                value_type: ValueType::Boolean,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Value, false),
        "bool p3_0 = default;\nif (condition) {\n    p3_0 = true;\n}\nreturn p3_0;"
    );
}

#[test]
fn missing_phi_definition_gets_a_conservative_default() {
    let body = Block::from(vec![Stmt::ret(Expr::var("p5_1"))]);
    let symbols = BTreeMap::from([(
        "p5_1".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Phi,
            value_type: ValueType::Unknown,
        },
    )]);

    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(plan.declarations["p5_1"].csharp_type, "dynamic");
    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Value, false),
        "dynamic p5_1 = default;\nreturn p5_1;"
    );
    assert!(plan.issues.iter().any(|issue| {
        issue.kind == LoweringIssueKind::LostStackValue && issue.detail.contains("p5_1")
    }));
}

#[test]
fn typed_boundaries_bridge_incompatible_known_types() {
    let body = Block::from(vec![
        Stmt::assign("flag", Expr::int(1)),
        Stmt::assign(
            "static0",
            Expr::NewArray {
                length: Box::new(Expr::int(2)),
                element_type: Some(ValueType::Integer),
            },
        ),
    ]);
    let symbols = BTreeMap::from([
        (
            "flag".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Boolean,
            },
        ),
        (
            "static0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Static(0),
                value_type: ValueType::Array,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Void, false),
        "bool flag = (bool)(dynamic)(1);\nstatic0 = (object[])(dynamic)(new BigInteger[(int)(2)]);"
    );
}

#[test]
fn typed_boundaries_box_value_array_literals_into_object_arrays() {
    let body = Block::from(vec![Stmt::assign(
        "static0",
        Expr::Array(vec![Expr::int(1), Expr::int(2)]),
    )]);
    let symbols = BTreeMap::from([(
        "static0".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Static(0),
            value_type: ValueType::Array,
        },
    )]);
    let plan =
        plan_declarations(&body, &symbols, true).with_static_field_types(&BTreeMap::from([(
            "static0".to_string(),
            "object[]".to_string(),
        )]));

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Void, false),
        "static0 = new object[] { 1, 2 };"
    );
}

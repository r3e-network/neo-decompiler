use super::*;

#[test]
fn plans_overloads_and_calls_together() {
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "Overloads",
            "abi": { "methods": [
                {
                    "name": "transfer",
                    "parameters": [{ "name": "value", "type": "Integer" }],
                    "returntype": "Integer",
                    "offset": 0
                },
                {
                    "name": "transfer",
                    "parameters": [{ "name": "enabled", "type": "Boolean" }],
                    "returntype": "Integer",
                    "offset": 20
                },
                {
                    "name": "transfer",
                    "parameters": [{ "name": "value", "type": "Integer" }],
                    "returntype": "Integer",
                    "offset": 40
                }
            ] }
        }"#,
    )
    .expect("manifest parsed");
    let instructions = vec![
        Instruction::new(0, OpCode::Nop, None),
        Instruction::new(20, OpCode::Nop, None),
        Instruction::new(40, OpCode::Nop, None),
        Instruction::new(42, OpCode::Call, Some(Operand::Jump(-2))),
        Instruction::new(44, OpCode::Ret, None),
    ];
    let call_graph = CallGraph {
        methods: vec![
            MethodRef {
                offset: 0,
                name: "transfer".to_string(),
            },
            MethodRef {
                offset: 20,
                name: "transfer".to_string(),
            },
            MethodRef {
                offset: 40,
                name: "transfer".to_string(),
            },
        ],
        edges: vec![CallEdge {
            caller: MethodRef {
                offset: 40,
                name: "transfer".to_string(),
            },
            call_offset: 42,
            opcode: "CALL".to_string(),
            target: CallTarget::Internal {
                method: MethodRef {
                    offset: 40,
                    name: "transfer".to_string(),
                },
            },
        }],
    };
    let method_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "transfer", 1, ReturnBehavior::Value),
            method_contract(20, "transfer", 1, ReturnBehavior::Value),
            method_contract(40, "transfer", 1, ReturnBehavior::Value),
        ],
        static_collection_facts: BTreeMap::new(),
    };

    let plans = build_csharp_method_plans(
        &instructions,
        Some(&manifest),
        &call_graph,
        &method_contracts,
        &TypeInfo::default(),
        &[0, 20, 40],
    );

    assert_eq!(plans[0].emitted_name, "transfer");
    assert_eq!(plans[1].emitted_name, "transfer");
    assert_eq!(plans[2].emitted_name, "transfer_2");
    assert_eq!(plans.method_return_types_by_offset()[&40], "BigInteger");
    assert_eq!(
        plans[2].method_context.calls_by_offset[&42]
            .target
            .display_name(),
        "transfer_2"
    );

    let ambiguous_manifest = ContractManifest::from_json_str(
        r#"{
            "name": "Ambiguous",
            "abi": { "methods": [
                {
                    "name": "caller",
                    "parameters": [],
                    "returntype": "Void",
                    "offset": 0
                },
                {
                    "name": "left",
                    "parameters": [],
                    "returntype": "Void",
                    "offset": 20
                },
                {
                    "name": "right",
                    "parameters": [],
                    "returntype": "Void",
                    "offset": 20
                }
            ] }
        }"#,
    )
    .expect("manifest parsed");
    let ambiguous_instructions = vec![
        Instruction::new(0, OpCode::Call, Some(Operand::Jump(20))),
        Instruction::new(2, OpCode::Ret, None),
        Instruction::new(20, OpCode::Ret, None),
    ];
    let ambiguous_call_graph = CallGraph {
        methods: vec![
            MethodRef {
                offset: 0,
                name: "caller".to_string(),
            },
            MethodRef {
                offset: 20,
                name: "left".to_string(),
            },
        ],
        edges: vec![CallEdge {
            caller: MethodRef {
                offset: 0,
                name: "caller".to_string(),
            },
            call_offset: 0,
            opcode: "CALL".to_string(),
            target: CallTarget::Internal {
                method: MethodRef {
                    offset: 20,
                    name: "left".to_string(),
                },
            },
        }],
    };
    let ambiguous_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "caller", 0, ReturnBehavior::Void),
            method_contract(20, "left", 0, ReturnBehavior::Void),
        ],
        static_collection_facts: BTreeMap::new(),
    };
    let ambiguous_plans = build_csharp_method_plans(
        &ambiguous_instructions,
        Some(&ambiguous_manifest),
        &ambiguous_call_graph,
        &ambiguous_contracts,
        &TypeInfo::default(),
        &[0, 20],
    );
    let ambiguous = &ambiguous_plans[0];

    assert!(matches!(
        ambiguous.method_context.calls_by_offset[&0].target,
        SemanticCallTarget::Unresolved { .. }
    ));
    assert!(!ambiguous_plans
        .method_return_types_by_offset()
        .contains_key(&20));
    assert!(ambiguous
        .planning_issues
        .iter()
        .any(|issue| issue.kind == LoweringIssueKind::UnresolvedCall));
}

#[test]
fn infers_concrete_return_type_for_private_literal_helper() {
    let instructions = vec![
        Instruction::new(0, OpCode::Call, Some(Operand::Jump(4))),
        Instruction::new(2, OpCode::Ret, None),
        Instruction::new(4, OpCode::Push1, None),
        Instruction::new(5, OpCode::Ret, None),
    ];
    let helper = MethodRef {
        offset: 4,
        name: "sub_0x0004".to_string(),
    };
    let call_graph = CallGraph {
        methods: vec![
            MethodRef {
                offset: 0,
                name: "sub_0x0000".to_string(),
            },
            helper.clone(),
        ],
        edges: vec![CallEdge {
            caller: MethodRef {
                offset: 0,
                name: "sub_0x0000".to_string(),
            },
            call_offset: 0,
            opcode: "CALL".to_string(),
            target: CallTarget::Internal { method: helper },
        }],
    };
    let method_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "sub_0x0000", 0, ReturnBehavior::Unknown),
            method_contract(4, "sub_0x0004", 0, ReturnBehavior::Unknown),
        ],
        static_collection_facts: BTreeMap::new(),
    };

    let plans = build_csharp_method_plans(
        &instructions,
        None,
        &call_graph,
        &method_contracts,
        &TypeInfo::default(),
        &[0, 4],
    );
    let helper_plan = plans.inferred_method(4).expect("private helper plan");

    assert_eq!(helper_plan.return_type, "BigInteger");
    assert_eq!(helper_plan.return_behavior, ReturnBehavior::Value);
    assert_eq!(plans.method_return_types_by_offset()[&4], "BigInteger");
}

#[test]
fn unresolved_private_call_keeps_helper_return_dynamic() {
    let instructions = vec![
        Instruction::new(0, OpCode::Call, Some(Operand::Jump(4))),
        Instruction::new(2, OpCode::Ret, None),
        Instruction::new(4, OpCode::Call, Some(Operand::Jump(4))),
        Instruction::new(6, OpCode::Ret, None),
    ];
    let entry = MethodRef {
        offset: 0,
        name: "sub_0x0000".to_string(),
    };
    let helper = MethodRef {
        offset: 4,
        name: "sub_0x0004".to_string(),
    };
    let call_graph = CallGraph {
        methods: vec![entry.clone(), helper.clone()],
        edges: vec![
            CallEdge {
                caller: entry,
                call_offset: 0,
                opcode: "CALL".to_string(),
                target: CallTarget::Internal {
                    method: helper.clone(),
                },
            },
            CallEdge {
                caller: helper,
                call_offset: 4,
                opcode: "CALL".to_string(),
                target: CallTarget::UnresolvedInternal { target: 8 },
            },
        ],
    };
    let method_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "sub_0x0000", 0, ReturnBehavior::Unknown),
            method_contract(4, "sub_0x0004", 0, ReturnBehavior::Unknown),
        ],
        static_collection_facts: BTreeMap::new(),
    };

    let plans = build_csharp_method_plans(
        &instructions,
        None,
        &call_graph,
        &method_contracts,
        &TypeInfo::default(),
        &[0, 4],
    );

    assert_eq!(
        plans
            .inferred_method(4)
            .expect("private helper plan")
            .return_type,
        "dynamic"
    );
}

#[test]
fn propagates_private_return_type_through_helper_chain() {
    let instructions = vec![
        Instruction::new(0, OpCode::Call, Some(Operand::Jump(4))),
        Instruction::new(2, OpCode::Ret, None),
        Instruction::new(4, OpCode::Call, Some(Operand::Jump(4))),
        Instruction::new(6, OpCode::Ret, None),
        Instruction::new(8, OpCode::Push1, None),
        Instruction::new(9, OpCode::Ret, None),
    ];
    let entry = MethodRef {
        offset: 0,
        name: "sub_0x0000".to_string(),
    };
    let middle = MethodRef {
        offset: 4,
        name: "sub_0x0004".to_string(),
    };
    let leaf = MethodRef {
        offset: 8,
        name: "sub_0x0008".to_string(),
    };
    let call_graph = CallGraph {
        methods: vec![entry.clone(), middle.clone(), leaf.clone()],
        edges: vec![
            CallEdge {
                caller: entry,
                call_offset: 0,
                opcode: "CALL".to_string(),
                target: CallTarget::Internal {
                    method: middle.clone(),
                },
            },
            CallEdge {
                caller: middle,
                call_offset: 4,
                opcode: "CALL".to_string(),
                target: CallTarget::Internal {
                    method: leaf.clone(),
                },
            },
        ],
    };
    let method_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "sub_0x0000", 0, ReturnBehavior::Unknown),
            method_contract(4, "sub_0x0004", 0, ReturnBehavior::Unknown),
            method_contract(8, "sub_0x0008", 0, ReturnBehavior::Unknown),
        ],
        static_collection_facts: BTreeMap::new(),
    };

    let plans = build_csharp_method_plans(
        &instructions,
        None,
        &call_graph,
        &method_contracts,
        &TypeInfo::default(),
        &[0, 4, 8],
    );

    assert_eq!(plans.inferred_method(8).unwrap().return_type, "BigInteger");
    assert_eq!(plans.inferred_method(4).unwrap().return_type, "BigInteger");
}

#[test]
fn mixed_private_returns_remain_dynamic() {
    let instructions = vec![
        Instruction::new(0, OpCode::Call, Some(Operand::Jump(4))),
        Instruction::new(2, OpCode::Ret, None),
        Instruction::new(4, OpCode::Push1, None),
        Instruction::new(5, OpCode::Jmpif, Some(Operand::Jump(4))),
        Instruction::new(7, OpCode::Push1, None),
        Instruction::new(8, OpCode::Ret, None),
        Instruction::new(9, OpCode::PushT, None),
        Instruction::new(10, OpCode::Ret, None),
    ];
    let entry = MethodRef {
        offset: 0,
        name: "sub_0x0000".to_string(),
    };
    let helper = MethodRef {
        offset: 4,
        name: "sub_0x0004".to_string(),
    };
    let call_graph = CallGraph {
        methods: vec![entry.clone(), helper.clone()],
        edges: vec![CallEdge {
            caller: entry,
            call_offset: 0,
            opcode: "CALL".to_string(),
            target: CallTarget::Internal { method: helper },
        }],
    };
    let method_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "sub_0x0000", 0, ReturnBehavior::Unknown),
            method_contract(4, "sub_0x0004", 0, ReturnBehavior::Unknown),
        ],
        static_collection_facts: BTreeMap::new(),
    };

    let plans = build_csharp_method_plans(
        &instructions,
        None,
        &call_graph,
        &method_contracts,
        &TypeInfo::default(),
        &[0, 4],
    );

    assert_eq!(plans.inferred_method(4).unwrap().return_type, "dynamic");
}

#[test]
fn plans_cross_range_tail_jump_with_detached_helper_arity() {
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "TailThunk",
            "abi": { "methods": [{
                "name": "setValue",
                "parameters": [{ "name": "value", "type": "Integer" }],
                "returntype": "Void",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");
    let instructions = vec![
        Instruction::new(0, OpCode::Push5, None),
        Instruction::new(1, OpCode::Jmp, Some(Operand::Jump(19))),
        Instruction::new(20, OpCode::Push1, None),
        Instruction::new(21, OpCode::Rot, None),
        Instruction::new(22, OpCode::Setitem, None),
        Instruction::new(23, OpCode::Ret, None),
    ];

    let plans = build_csharp_method_plans(
        &instructions,
        Some(&manifest),
        &CallGraph::default(),
        &MethodContracts::default(),
        &TypeInfo::default(),
        &[0, 20],
    );
    let thunk = plans.manifest_method(0);
    let helper = plans.inferred_method(20).expect("detached helper plan");

    assert!(thunk.method_context.arguments_on_entry_stack);
    assert_eq!(helper.parameters.len(), 2);
    let tail = &thunk.method_context.calls_by_offset[&1];
    assert_eq!(tail.argument_count, 2);
    assert!(!tail.returns_value);
    assert!(matches!(
        &tail.target,
        SemanticCallTarget::Internal { offset: 20, name }
            if name == &helper.emitted_name
    ));
}

#[test]
fn null_checked_value_parameters_use_dynamic_csharp_signatures() {
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "NullableParameter",
            "abi": { "methods": [{
                "name": "valueOrDefault",
                "parameters": [{ "name": "value", "type": "Integer" }],
                "returntype": "Integer",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");
    let instructions = vec![
        Instruction::new(0, OpCode::Initslot, Some(Operand::Bytes(vec![0, 1]))),
        Instruction::new(3, OpCode::Ldarg0, None),
        Instruction::new(4, OpCode::Dup, None),
        Instruction::new(5, OpCode::Isnull, None),
        Instruction::new(6, OpCode::Ret, None),
    ];

    let plans = build_csharp_method_plans(
        &instructions,
        Some(&manifest),
        &CallGraph::default(),
        &MethodContracts::default(),
        &TypeInfo::default(),
        &[0],
    );

    assert_eq!(plans.manifest_method(0).parameters[0].ty, "dynamic");
}

#[test]
fn null_checked_local_aliases_use_dynamic_csharp_signatures() {
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "NullableAlias",
            "abi": { "methods": [{
                "name": "valueOrDefault",
                "parameters": [{ "name": "value", "type": "Integer" }],
                "returntype": "Integer",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");
    let instructions = vec![
        Instruction::new(0, OpCode::Initslot, Some(Operand::Bytes(vec![1, 1]))),
        Instruction::new(3, OpCode::Ldarg0, None),
        Instruction::new(4, OpCode::Stloc0, None),
        Instruction::new(5, OpCode::Ldloc0, None),
        Instruction::new(6, OpCode::Isnull, None),
        Instruction::new(7, OpCode::Ret, None),
    ];
    let plans = build_csharp_method_plans(
        &instructions,
        Some(&manifest),
        &CallGraph::default(),
        &MethodContracts::default(),
        &TypeInfo::default(),
        &[0],
    );

    assert_eq!(plans.manifest_method(0).parameters[0].ty, "dynamic");
}

#[test]
fn plans_declarations() {
    let body = Block::with_stmts(vec![
        Stmt::assign("t_0", Expr::int(1)),
        Stmt::expr(Expr::var("t_0")),
        Stmt::ControlFlow(Box::new(ControlFlow::if_else(
            Expr::var("cond"),
            Block::with_stmts(vec![Stmt::assign("loc0", Expr::int(1))]),
            Block::with_stmts(vec![Stmt::assign("loc0", Expr::int(2))]),
        ))),
        Stmt::expr(Expr::var("loc0")),
        Stmt::assign("loc1", Expr::int(0)),
        Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
            Expr::binary(BinOp::Lt, Expr::var("loc1"), Expr::int(3)),
            Block::with_stmts(vec![Stmt::assign(
                "loc1",
                Expr::binary(BinOp::Add, Expr::var("loc1"), Expr::int(1)),
            )]),
        ))),
        Stmt::expr(Expr::var("loc1")),
        Stmt::assign("static1", Expr::var("loc0")),
        Stmt::expr(Expr::var("static1")),
        Stmt::expr(Expr::var("@class")),
        Stmt::expr(Expr::var("missing")),
    ]);
    let symbols = BTreeMap::from([
        (
            "cond".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Boolean,
            },
        ),
        (
            "@class".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(1),
                value_type: ValueType::Any,
            },
        ),
        (
            "loc0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Integer,
            },
        ),
        (
            "loc1".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(1),
                value_type: ValueType::Integer,
            },
        ),
        (
            "static1".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Static(1),
                value_type: ValueType::Integer,
            },
        ),
        (
            "t_0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "missing".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "static3".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Static(3),
                value_type: ValueType::Boolean,
            },
        ),
    ]);

    let plan = plan_declarations(&body, &symbols, true);
    let root_scope = plan.scopes.root();

    assert_eq!(plan.declarations["t_0"].scope, root_scope);
    assert_eq!(plan.declarations["t_0"].kind, DeclarationKind::Inline);
    assert_eq!(plan.declarations["loc0"].scope, root_scope);
    assert_eq!(
        plan.declarations["loc0"].kind,
        DeclarationKind::HoistedAssignment
    );
    assert_eq!(plan.declarations["loc1"].scope, root_scope);
    assert_eq!(
        plan.declarations["loc1"].kind,
        DeclarationKind::HoistedAssignment
    );
    assert!(!plan.declarations.contains_key("cond"));
    assert!(!plan.declarations.contains_key("@class"));
    assert!(!plan.declarations.contains_key("static1"));
    assert!(plan.issues.iter().any(|issue| {
        issue.kind == LoweringIssueKind::LostStackValue && issue.detail.contains("missing")
    }));

    let types = TypeInfo {
        statics: vec![ValueType::Unknown, ValueType::Integer],
        ..TypeInfo::default()
    };
    let contract = plan_contract_symbols(&types, &[&symbols], true, &BTreeSet::new());
    assert_eq!(contract.static_fields[1].name, "static1");
    assert_eq!(contract.static_fields[1].csharp_type, "BigInteger");
    let static3 = contract
        .static_fields
        .iter()
        .find(|field| field.name == "static3")
        .expect("referenced static beyond TypeInfo is planned");
    assert_eq!(static3.csharp_type, "bool");
}

#[test]
fn for_body_definition_used_by_update_is_hoisted_to_the_loop_scope() {
    let body = Block::with_stmts(vec![Stmt::ControlFlow(Box::new(ControlFlow::for_loop(
        None,
        Some(Expr::int(1)),
        Some(Expr::var("body_value")),
        Block::with_stmts(vec![Stmt::assign("body_value", Expr::int(1))]),
    )))]);
    let symbols = BTreeMap::from([(
        "body_value".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Integer,
        },
    )]);

    let plan = plan_declarations(&body, &symbols, true);
    let declaration = &plan.declarations["body_value"];

    assert_ne!(declaration.scope, plan.scopes.root());
    assert_eq!(declaration.kind, DeclarationKind::HoistedAssignment);
}

#[test]
fn missing_uninitialized_symbol_is_a_lost_stack_value() {
    let plan = plan_declarations(
        &Block::with_stmts(vec![Stmt::expr(Expr::var("orphan"))]),
        &BTreeMap::new(),
        true,
    );

    assert!(plan.issues.iter().any(|issue| {
        issue.kind == LoweringIssueKind::LostStackValue && issue.detail.contains("orphan")
    }));
    assert!(!plan
        .issues
        .iter()
        .any(|issue| issue.kind == LoweringIssueKind::MissingProvenance));
}

#[test]
fn stack_placeholder_is_a_lost_stack_value() {
    let plan = plan_declarations(
        &Block::with_stmts(vec![Stmt::expr(Expr::StackTemp(7))]),
        &BTreeMap::new(),
        true,
    );

    assert!(plan.issues.iter().any(|issue| {
        issue.kind == LoweringIssueKind::LostStackValue && issue.detail.contains('7')
    }));
    assert!(plan.declarations.is_empty());
}

#[test]
fn csharp_emits_static_referenced_beyond_type_info() {
    let instructions = vec![
        Instruction::new(0, OpCode::Ldsfld3, None),
        Instruction::new(1, OpCode::Ret, None),
    ];
    let nef = NefFile {
        header: NefHeader {
            magic: *b"NEF3",
            compiler: "test".to_string(),
            source: String::new(),
        },
        method_tokens: Vec::new(),
        script: vec![0x5B, 0x40],
        checksum: 0,
    };
    let rendered = super::super::super::render_csharp(
        &nef,
        &instructions,
        None,
        &CallGraph::default(),
        &MethodContracts::default(),
        &TypeInfo::default(),
        &RenderOptions {
            typed_declarations: true,
            ..RenderOptions::default()
        },
    )
    .source;

    assert!(
        rendered.contains("private static dynamic static3;"),
        "referenced static beyond TypeInfo must be emitted at class scope: {rendered}"
    );
}

#[test]
fn referenced_static_beyond_type_info_reserves_the_method_name() {
    let instructions = vec![
        Instruction::new(0, OpCode::Ldsfld3, None),
        Instruction::new(1, OpCode::Ret, None),
    ];
    let nef = NefFile {
        header: NefHeader {
            magic: *b"NEF3",
            compiler: "test".to_string(),
            source: String::new(),
        },
        method_tokens: Vec::new(),
        script: vec![0x5B, 0x40],
        checksum: 0,
    };
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "StaticCollision",
            "abi": { "methods": [{
                "name": "static3",
                "parameters": [],
                "returntype": "Any",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");
    let rendered = super::super::super::render_csharp(
        &nef,
        &instructions,
        Some(&manifest),
        &CallGraph::default(),
        &MethodContracts::default(),
        &TypeInfo::default(),
        &RenderOptions {
            typed_declarations: true,
            ..RenderOptions::default()
        },
    )
    .source;

    assert!(rendered.contains("private static dynamic static3;"));
    assert!(
        rendered.contains("public static object static3_1()"),
        "referenced static names must be reserved before methods are named: {rendered}"
    );
}

#[test]
fn static_fields_reserve_contract_member_names() {
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "StaticCollision",
            "abi": { "methods": [{
                "name": "static0",
                "parameters": [],
                "returntype": "Void",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");
    let types = TypeInfo {
        statics: vec![ValueType::Integer],
        ..TypeInfo::default()
    };

    let plans = build_csharp_method_plans(
        &[Instruction::new(0, OpCode::Ret, None)],
        Some(&manifest),
        &CallGraph::default(),
        &MethodContracts::default(),
        &types,
        &[0],
    );

    assert_eq!(plans[0].emitted_name, "static0_1");
}

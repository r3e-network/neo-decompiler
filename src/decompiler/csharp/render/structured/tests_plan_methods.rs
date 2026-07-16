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
fn infers_private_parameter_type_from_unanimous_internal_calls() {
    let instructions = vec![
        Instruction::new(0, OpCode::Push1, None),
        Instruction::new(1, OpCode::Call, Some(Operand::Jump(3))),
        Instruction::new(2, OpCode::Ret, None),
        Instruction::new(4, OpCode::Ldarg0, None),
        Instruction::new(5, OpCode::Ret, None),
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
            call_offset: 1,
            opcode: "CALL".to_string(),
            target: CallTarget::Internal { method: helper },
        }],
    };
    let method_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "sub_0x0000", 0, ReturnBehavior::Unknown),
            method_contract(4, "sub_0x0004", 1, ReturnBehavior::Unknown),
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
    assert_eq!(helper_plan.parameters[0].ty, "BigInteger");
    assert_eq!(helper_plan.symbol_types.parameters[0], ValueType::Integer);
}

#[test]
fn conflicting_private_parameter_calls_remain_dynamic() {
    let instructions = vec![
        Instruction::new(0, OpCode::Push1, None),
        Instruction::new(1, OpCode::Call, Some(Operand::Jump(9))),
        Instruction::new(2, OpCode::PushT, None),
        Instruction::new(3, OpCode::Call, Some(Operand::Jump(7))),
        Instruction::new(4, OpCode::Ret, None),
        Instruction::new(10, OpCode::Ldarg0, None),
        Instruction::new(11, OpCode::Ret, None),
    ];
    let entry = MethodRef {
        offset: 0,
        name: "sub_0x0000".to_string(),
    };
    let helper = MethodRef {
        offset: 10,
        name: "sub_0x000A".to_string(),
    };
    let call_graph = CallGraph {
        methods: vec![entry.clone(), helper.clone()],
        edges: vec![
            CallEdge {
                caller: entry.clone(),
                call_offset: 1,
                opcode: "CALL".to_string(),
                target: CallTarget::Internal {
                    method: helper.clone(),
                },
            },
            CallEdge {
                caller: entry,
                call_offset: 3,
                opcode: "CALL".to_string(),
                target: CallTarget::Internal { method: helper },
            },
        ],
    };
    let method_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "sub_0x0000", 0, ReturnBehavior::Unknown),
            method_contract(10, "sub_0x000A", 1, ReturnBehavior::Unknown),
        ],
        static_collection_facts: BTreeMap::new(),
    };

    let plans = build_csharp_method_plans(
        &instructions,
        None,
        &call_graph,
        &method_contracts,
        &TypeInfo::default(),
        &[0, 10],
    );

    assert_eq!(
        plans
            .inferred_method(10)
            .expect("private helper plan")
            .parameters[0]
            .ty,
        "dynamic"
    );
}

#[test]
fn null_checked_private_parameter_stays_dynamic() {
    let instructions = vec![
        Instruction::new(0, OpCode::Push1, None),
        Instruction::new(1, OpCode::Call, Some(Operand::Jump(3))),
        Instruction::new(2, OpCode::Ret, None),
        Instruction::new(4, OpCode::Ldarg0, None),
        Instruction::new(5, OpCode::Dup, None),
        Instruction::new(6, OpCode::Isnull, None),
        Instruction::new(7, OpCode::Ret, None),
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
            call_offset: 1,
            opcode: "CALL".to_string(),
            target: CallTarget::Internal { method: helper },
        }],
    };
    let method_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "sub_0x0000", 0, ReturnBehavior::Unknown),
            method_contract(4, "sub_0x0004", 1, ReturnBehavior::Unknown),
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
            .parameters[0]
            .ty,
        "dynamic"
    );
}

#[test]
fn null_checked_private_array_parameter_uses_the_proven_reference_type() {
    let instructions = vec![
        Instruction::new(0, OpCode::Newarray0, None),
        Instruction::new(1, OpCode::Call, Some(Operand::Jump(3))),
        Instruction::new(2, OpCode::Ret, None),
        Instruction::new(4, OpCode::Ldarg0, None),
        Instruction::new(5, OpCode::Dup, None),
        Instruction::new(6, OpCode::Isnull, None),
        Instruction::new(7, OpCode::Ret, None),
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
            call_offset: 1,
            opcode: "CALL".to_string(),
            target: CallTarget::Internal { method: helper },
        }],
    };
    let method_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "sub_0x0000", 0, ReturnBehavior::Unknown),
            method_contract(4, "sub_0x0004", 1, ReturnBehavior::Unknown),
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
    assert_eq!(helper_plan.parameters[0].ty, "object[]");
    assert_eq!(helper_plan.symbol_types.parameters[0], ValueType::Array);
}

#[test]
fn indexed_private_parameter_stays_dynamic() {
    let instructions = vec![
        Instruction::new(0, OpCode::Push1, None),
        Instruction::new(1, OpCode::Call, Some(Operand::Jump(3))),
        Instruction::new(2, OpCode::Ret, None),
        Instruction::new(4, OpCode::Ldarg0, None),
        Instruction::new(5, OpCode::Push0, None),
        Instruction::new(6, OpCode::Pickitem, None),
        Instruction::new(7, OpCode::Ret, None),
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
            call_offset: 1,
            opcode: "CALL".to_string(),
            target: CallTarget::Internal { method: helper },
        }],
    };
    let method_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "sub_0x0000", 0, ReturnBehavior::Unknown),
            method_contract(4, "sub_0x0004", 1, ReturnBehavior::Unknown),
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
            .parameters[0]
            .ty,
        "dynamic"
    );
}

#[test]
fn indexed_private_array_parameter_uses_the_proven_array_type() {
    let instructions = vec![
        Instruction::new(0, OpCode::Newarray0, None),
        Instruction::new(1, OpCode::Call, Some(Operand::Jump(3))),
        Instruction::new(2, OpCode::Ret, None),
        Instruction::new(4, OpCode::Ldarg0, None),
        Instruction::new(5, OpCode::Push0, None),
        Instruction::new(6, OpCode::Pickitem, None),
        Instruction::new(7, OpCode::Ret, None),
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
            call_offset: 1,
            opcode: "CALL".to_string(),
            target: CallTarget::Internal { method: helper },
        }],
    };
    let method_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "sub_0x0000", 0, ReturnBehavior::Unknown),
            method_contract(4, "sub_0x0004", 1, ReturnBehavior::Unknown),
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
    assert_eq!(helper_plan.parameters[0].ty, "object[]");
    assert_eq!(helper_plan.symbol_types.parameters[0], ValueType::Array);
}

#[test]
fn infers_exact_string_return_type_for_private_native_helper() {
    let instructions = vec![
        Instruction::new(0, OpCode::Call, Some(Operand::Jump(4))),
        Instruction::new(2, OpCode::Ret, None),
        Instruction::new(4, OpCode::CallT, Some(Operand::U16(0))),
        Instruction::new(7, OpCode::Ret, None),
    ];
    let entry = MethodRef {
        offset: 0,
        name: "sub_0x0000".to_string(),
    };
    let helper = MethodRef {
        offset: 4,
        name: "sub_0x0004".to_string(),
    };
    let token = CallTarget::MethodToken {
        index: 0,
        hash_le: "C0EF39CEE0E4E925C6C2A06A79E1440DD86FCEAC".to_string(),
        hash_be: "ACCE6FD80D44E1796AA0C2C625E9E4E0CE39EFC0".to_string(),
        method: "base58CheckEncode".to_string(),
        parameters_count: 1,
        has_return_value: true,
        call_flags: 0x0F,
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
                opcode: "CALLT".to_string(),
                target: token,
            },
        ],
    };
    let method_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "sub_0x0000", 0, ReturnBehavior::Unknown),
            method_contract(4, "sub_0x0004", 1, ReturnBehavior::Unknown),
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

    assert_eq!(plans.inferred_method(4).unwrap().return_type, "string");
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

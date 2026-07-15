use super::*;

#[test]
fn resolved_internal_call_return_types_drive_expression_typing() {
    let call = |offset| {
        Expr::call(
            SemanticCallTarget::Internal {
                offset,
                name: format!("helper_{offset}"),
            },
            Vec::new(),
        )
    };
    let context = ExprContext::default().with_internal_call_return_types(&BTreeMap::from([
        (1, "BigInteger".to_string()),
        (2, "bool".to_string()),
        (3, "ByteString".to_string()),
        (4, "byte[]".to_string()),
        (5, "Map<object, object>".to_string()),
        (6, "object[]".to_string()),
        (7, "dynamic".to_string()),
    ]));

    assert_eq!(context.value_type(&call(1)), ValueType::Integer);
    assert_eq!(context.value_type(&call(2)), ValueType::Boolean);
    assert_eq!(context.value_type(&call(3)), ValueType::ByteString);
    assert_eq!(context.value_type(&call(4)), ValueType::Buffer);
    assert_eq!(context.value_type(&call(5)), ValueType::Map);
    assert_eq!(context.value_type(&call(6)), ValueType::Array);
    assert_eq!(context.value_type(&call(7)), ValueType::Unknown);
    assert_eq!(context.value_type(&call(8)), ValueType::Unknown);

    let unresolved = Expr::unresolved_call("helper", Vec::new());
    assert_eq!(context.value_type(&unresolved), ValueType::Unknown);
}

#[test]
fn resolved_boolean_internal_call_avoids_dynamic_truthiness_cast() {
    let call = Expr::call(
        SemanticCallTarget::Internal {
            offset: 42,
            name: "predicate".to_string(),
        },
        Vec::new(),
    );
    let context = ExprContext::default()
        .with_internal_call_return_types(&BTreeMap::from([(42, "bool".to_string())]));

    assert_eq!(render_vm_condition(&call, &context), "predicate()");
    assert_eq!(
        render_expr(&Expr::unary(UnaryOp::LogicalNot, call), &context),
        "!predicate()"
    );
}

#[test]
fn known_native_method_tokens_drive_exact_csharp_expression_types() {
    let context = ExprContext::default();
    let call = |name: &str| {
        Expr::call(
            SemanticCallTarget::MethodToken {
                index: 0,
                name: name.to_string(),
                hash_le: Some("C0EF39CEE0E4E925C6C2A06A79E1440DD86FCEAC".to_string()),
                call_flags: Some(0x0F),
            },
            Vec::new(),
        )
    };

    let string = call("base58CheckEncode");
    assert_eq!(context.exact_csharp_type(&string), Some("string"));
    assert_eq!(context.value_type(&string), ValueType::ByteString);

    let integer = call("strLen");
    assert_eq!(context.exact_csharp_type(&integer), Some("BigInteger"));
    assert_eq!(context.value_type(&integer), ValueType::Integer);

    let restricted = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 0,
            name: "strLen".to_string(),
            hash_le: Some("C0EF39CEE0E4E925C6C2A06A79E1440DD86FCEAC".to_string()),
            call_flags: Some(0x01),
        },
        Vec::new(),
    );
    assert_eq!(context.exact_csharp_type(&restricted), None);
    assert_eq!(context.value_type(&restricted), ValueType::Unknown);
}

#[test]
fn framework_native_alias_types_remain_concrete_in_expression_context() {
    let context = ExprContext::default();
    let call = |hash: &str, name: &str| {
        Expr::call(
            SemanticCallTarget::MethodToken {
                index: 0,
                name: name.to_string(),
                hash_le: Some(hash.to_string()),
                call_flags: Some(0x0F),
            },
            Vec::new(),
        )
    };

    let current_index = call("BEF2043140362A77C15099C7E64C12F700B665DA", "currentIndex");
    assert_eq!(context.exact_csharp_type(&current_index), Some("uint"));
    assert_eq!(context.value_type(&current_index), ValueType::Integer);

    let signers = call(
        "BEF2043140362A77C15099C7E64C12F700B665DA",
        "getTransactionSigners",
    );
    assert_eq!(context.exact_csharp_type(&signers), Some("Signer[]"));
    assert_eq!(context.value_type(&signers), ValueType::Array);

    let designated = call(
        "E295E391544C178AD94F03EC4DCDFF78534ECF49",
        "getDesignatedByRole",
    );
    assert_eq!(context.exact_csharp_type(&designated), Some("ECPoint[]"));
    assert_eq!(context.value_type(&designated), ValueType::Array);
}

#[test]
fn additional_framework_returns_remain_concrete_in_expression_context() {
    let context = ExprContext::default();
    let call = |hash: &str, name: &str| {
        Expr::call(
            SemanticCallTarget::MethodToken {
                index: 0,
                name: name.to_string(),
                hash_le: Some(hash.to_string()),
                call_flags: Some(0x0F),
            },
            Vec::new(),
        )
    };

    let contract = call("FDA3FA4346EA532A258FC497DDADDB6437C9FDFF", "getContract");
    assert_eq!(context.exact_csharp_type(&contract), Some("Contract"));
    assert_eq!(context.value_type(&contract), ValueType::InteropInterface);

    let block = call("BEF2043140362A77C15099C7E64C12F700B665DA", "getBlock");
    assert_eq!(context.exact_csharp_type(&block), Some("Block"));
    assert_eq!(context.value_type(&block), ValueType::InteropInterface);

    let candidates = call("F563EA40BC283D4D0E05C48EA305B3F2A07340EF", "getCandidates");
    assert_eq!(
        context.exact_csharp_type(&candidates),
        Some("(ECPoint, BigInteger)[]")
    );
    assert_eq!(context.value_type(&candidates), ValueType::Array);

    let candidate_iterator = call(
        "F563EA40BC283D4D0E05C48EA305B3F2A07340EF",
        "getAllCandidates",
    );
    assert_eq!(
        context.exact_csharp_type(&candidate_iterator),
        Some("Iterator<(ECPoint, BigInteger)>")
    );
    assert_eq!(
        context.value_type(&candidate_iterator),
        ValueType::InteropInterface
    );

    let account_state = call(
        "F563EA40BC283D4D0E05C48EA305B3F2A07340EF",
        "getAccountState",
    );
    assert_eq!(
        context.exact_csharp_type(&account_state),
        Some("NeoAccountState")
    );
    assert_eq!(
        context.value_type(&account_state),
        ValueType::InteropInterface
    );
}

#[test]
fn native_array_returns_preserve_index_element_types() {
    let context = ExprContext::default();
    let call = |hash: &str, name: &str| {
        Expr::call(
            SemanticCallTarget::MethodToken {
                index: 0,
                name: name.to_string(),
                hash_le: Some(hash.to_string()),
                call_flags: Some(0x0F),
            },
            Vec::new(),
        )
    };
    let candidates = call("F563EA40BC283D4D0E05C48EA305B3F2A07340EF", "getCandidates");
    assert_eq!(
        context.value_type(&Expr::index(candidates.clone(), Expr::int(0))),
        ValueType::Struct
    );
    assert_eq!(
        context.exact_csharp_type(&Expr::index(candidates.clone(), Expr::int(0))),
        Some("(ECPoint, BigInteger)")
    );

    let pickitem = Expr::call(
        SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Pickitem)),
        vec![candidates, Expr::int(0)],
    );
    assert_eq!(context.value_type(&pickitem), ValueType::Struct);
    assert_eq!(
        context.exact_csharp_type(&pickitem),
        Some("(ECPoint, BigInteger)")
    );

    let signers = call(
        "BEF2043140362A77C15099C7E64C12F700B665DA",
        "getTransactionSigners",
    );
    assert_eq!(
        context.value_type(&Expr::index(signers, Expr::int(0))),
        ValueType::InteropInterface
    );
}

#[test]
fn framework_object_members_preserve_concrete_types() {
    let context = ExprContext::default();
    let syscall = |hash| Expr::call(SemanticCallTarget::Syscall { hash, name: None }, Vec::new());
    let transaction = Expr::Member {
        base: Box::new(syscall(0x3008_512D)),
        name: "Sender".to_string(),
    };
    assert_eq!(context.exact_csharp_type(&transaction), Some("UInt160"));
    assert_eq!(context.value_type(&transaction), ValueType::ByteString);

    let notifications = Expr::index(syscall(0xF135_4327), Expr::int(0));
    let state = Expr::Member {
        base: Box::new(notifications),
        name: "State".to_string(),
    };
    assert_eq!(context.exact_csharp_type(&state), Some("object[]"));
    assert_eq!(context.value_type(&state), ValueType::Array);

    let signers = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 0,
            name: "getTransactionSigners".to_string(),
            hash_le: Some("BEF2043140362A77C15099C7E64C12F700B665DA".to_string()),
            call_flags: Some(0x0F),
        },
        Vec::new(),
    );
    let account = Expr::Member {
        base: Box::new(Expr::index(signers, Expr::int(0))),
        name: "Account".to_string(),
    };
    assert_eq!(context.exact_csharp_type(&account), Some("UInt160"));
    assert_eq!(context.value_type(&account), ValueType::ByteString);
}

#[test]
fn validated_csharp_variable_types_refine_unknown_value_types() {
    let context = ExprContext::default().with_concrete_types(&BTreeMap::from([
        ("storage".to_string(), "StorageContext".to_string()),
        ("key".to_string(), "ByteString".to_string()),
        ("amount".to_string(), "BigInteger".to_string()),
        ("unknown".to_string(), "dynamic".to_string()),
        ("opaque".to_string(), "object".to_string()),
    ]));

    assert_eq!(
        context.value_type(&Expr::var("storage")),
        ValueType::InteropInterface
    );
    assert_eq!(context.value_type(&Expr::var("key")), ValueType::ByteString);
    assert_eq!(context.value_type(&Expr::var("amount")), ValueType::Integer);
    assert_eq!(
        context.value_type(&Expr::var("unknown")),
        ValueType::Unknown
    );
    assert_eq!(context.value_type(&Expr::var("opaque")), ValueType::Unknown);
}

#[test]
fn planned_alias_types_flow_into_member_and_index_inference() {
    let get_contract = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 0,
            name: "getContract".to_string(),
            hash_le: Some("FDA3FA4346EA532A258FC497DDADDB6437C9FDFF".to_string()),
            call_flags: Some(0x0F),
        },
        vec![Expr::var("account")],
    );
    let body = Block::from(vec![
        Stmt::assign("contract", get_contract),
        Stmt::assign(
            "hash",
            Expr::Member {
                base: Box::new(Expr::var("contract")),
                name: "Hash".to_string(),
            },
        ),
        Stmt::Return(Some(Expr::var("hash"))),
    ]);
    let symbols = BTreeMap::from([
        (
            "account".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::ByteString,
            },
        ),
        (
            "contract".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::InteropInterface,
            },
        ),
        (
            "hash".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::ByteString,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);
    let rendered =
        super::super::stmt::render_block(&body, &plan, &symbols, ReturnBehavior::Value, false);

    assert!(
        rendered.contains("Contract contract = ContractManagement.GetContract(account);"),
        "{rendered}"
    );
    assert!(
        rendered.contains("UInt160 hash = contract.Hash;"),
        "{rendered}"
    );
}

#[test]
fn planned_parameter_types_flow_into_member_inference() {
    let body = Block::from(vec![
        Stmt::assign(
            "hash",
            Expr::Member {
                base: Box::new(Expr::var("tx")),
                name: "Hash".to_string(),
            },
        ),
        Stmt::Return(Some(Expr::var("hash"))),
    ]);
    let symbols = BTreeMap::from([
        (
            "tx".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::InteropInterface,
            },
        ),
        (
            "hash".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::ByteString,
            },
        ),
    ]);
    let parameter_types = BTreeMap::from([("tx".to_string(), "Transaction".to_string())]);
    let plan = super::super::plan::plan_declarations_with_known_types(
        &body,
        &symbols,
        true,
        &parameter_types,
    );
    let rendered =
        super::super::stmt::render_block(&body, &plan, &symbols, ReturnBehavior::Value, false);

    assert!(rendered.contains("UInt256 hash = tx.Hash;"), "{rendered}");
}

#[test]
fn known_syscalls_drive_exact_csharp_expression_types() {
    let context = ExprContext::default();
    let syscall = |hash| Expr::call(SemanticCallTarget::Syscall { hash, name: None }, Vec::new());

    let time = syscall(0x0388_C3B7);
    assert_eq!(context.exact_csharp_type(&time), Some("ulong"));
    assert_eq!(context.value_type(&time), ValueType::Integer);

    let trigger = syscall(0xA038_7DE9);
    assert_eq!(context.exact_csharp_type(&trigger), Some("TriggerType"));
    assert_eq!(context.value_type(&trigger), ValueType::Integer);

    let signers = syscall(0x8B18_F1AC);
    assert_eq!(context.exact_csharp_type(&signers), Some("Signer[]"));
    assert_eq!(context.value_type(&signers), ValueType::Array);

    let notifications = syscall(0xF135_4327);
    assert_eq!(
        context.exact_csharp_type(&notifications),
        Some("Notification[]")
    );
    assert_eq!(context.value_type(&notifications), ValueType::Array);

    let call_flags = syscall(0x813A_DA95);
    assert_eq!(context.exact_csharp_type(&call_flags), Some("CallFlags"));
    assert_eq!(context.value_type(&call_flags), ValueType::Integer);

    let storage = syscall(0x31E8_5D92);
    assert_eq!(context.exact_csharp_type(&storage), Some("ByteString"));
    assert_eq!(context.value_type(&storage), ValueType::ByteString);

    let iterator = syscall(0x9AB8_30DF);
    assert_eq!(context.exact_csharp_type(&iterator), Some("Iterator"));
    assert_eq!(context.value_type(&iterator), ValueType::InteropInterface);

    let unknown = syscall(0xDEAD_BEEF);
    assert_eq!(context.exact_csharp_type(&unknown), None);
    assert_eq!(context.value_type(&unknown), ValueType::Unknown);
}

#[test]
fn proven_expression_shapes_keep_concrete_value_types() {
    let context = ExprContext::default();
    let typed_array = Expr::NewArray {
        length: Box::new(Expr::int(2)),
        element_type: Some(ValueType::Integer),
    };
    assert_eq!(
        context.value_type(&Expr::index(typed_array, Expr::int(0))),
        ValueType::Integer
    );
    assert_eq!(
        context.value_type(&Expr::Ternary {
            condition: Box::new(Expr::var("condition")),
            then_expr: Box::new(Expr::int(1)),
            else_expr: Box::new(Expr::int(2)),
        }),
        ValueType::Integer
    );
    assert_eq!(
        context.value_type(&Expr::Ternary {
            condition: Box::new(Expr::var("condition")),
            then_expr: Box::new(Expr::int(1)),
            else_expr: Box::new(Expr::Literal(Literal::Bool(false))),
        }),
        ValueType::Unknown
    );
    assert_eq!(
        context.value_type(&Expr::Cast {
            expr: Box::new(Expr::Unknown),
            target_type: "BigInteger".to_string(),
        }),
        ValueType::Integer
    );
    assert_eq!(
        context.value_type(&Expr::Cast {
            expr: Box::new(Expr::Unknown),
            target_type: "object[]".to_string(),
        }),
        ValueType::Array
    );
}

#[test]
fn intrinsic_and_member_shapes_keep_concrete_value_types() {
    let context = expr_context_with_types(&[
        ("text", ValueType::ByteString),
        ("buffer", ValueType::Buffer),
        ("map", ValueType::Map),
    ]);
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };

    assert_eq!(
        context.value_type(&intrinsic(OpCode::Depth, Vec::new())),
        ValueType::Integer
    );
    assert_eq!(
        context.value_type(&intrinsic(
            OpCode::Substr,
            vec![Expr::var("text"), Expr::int(0), Expr::int(1)],
        )),
        ValueType::ByteString
    );
    assert_eq!(
        context.value_type(&intrinsic(
            OpCode::Left,
            vec![Expr::var("buffer"), Expr::int(1)],
        )),
        ValueType::Buffer
    );
    assert_eq!(
        context.value_type(&intrinsic(OpCode::Keys, vec![Expr::var("map")])),
        ValueType::Array
    );
    assert_eq!(
        context.value_type(&intrinsic(OpCode::Values, vec![Expr::var("map")])),
        ValueType::Array
    );
    assert_eq!(
        context.value_type(&intrinsic(
            OpCode::Pickitem,
            vec![Expr::var("text"), Expr::int(0)],
        )),
        ValueType::Integer
    );
    assert_eq!(
        context.value_type(&intrinsic(
            OpCode::Within,
            vec![Expr::int(1), Expr::int(0), Expr::int(2)],
        )),
        ValueType::Boolean
    );
    assert_eq!(
        context.value_type(&intrinsic(
            OpCode::Modpow,
            vec![Expr::int(2), Expr::int(3), Expr::int(5)],
        )),
        ValueType::Integer
    );
    assert_eq!(
        context.value_type(&Expr::Member {
            base: Box::new(Expr::var("text")),
            name: "Length".to_string(),
        }),
        ValueType::Integer
    );
    assert_eq!(
        context.value_type(&Expr::index(Expr::var("buffer"), Expr::int(0))),
        ValueType::Integer
    );
}

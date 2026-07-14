use super::*;

#[test]
fn infers_concrete_types_for_common_structured_expressions() {
    assert_eq!(
        concrete_definition_type(&Expr::int(1)),
        Some("BigInteger".to_string())
    );
    assert_eq!(
        concrete_definition_type(&Expr::Literal(Literal::String("text".to_string()))),
        Some("ByteString".to_string())
    );
    assert_eq!(
        concrete_definition_type(&Expr::binary(BinOp::Add, Expr::int(1), Expr::int(2))),
        Some("BigInteger".to_string())
    );
    assert_eq!(
        concrete_definition_type(&Expr::binary(BinOp::Eq, Expr::int(1), Expr::int(2))),
        Some("bool".to_string())
    );
    assert_eq!(
        concrete_definition_type(&Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Size)),
            vec![Expr::var("items")],
        )),
        Some("BigInteger".to_string())
    );
    assert_eq!(
        concrete_definition_type(&Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Cat)),
            vec![Expr::var("left"), Expr::var("right")],
        )),
        Some("ByteString".to_string())
    );
    assert_eq!(
        concrete_definition_type(&Expr::Member {
            base: Box::new(Expr::var("items")),
            name: "Length".to_string(),
        }),
        Some("BigInteger".to_string())
    );
    assert_eq!(concrete_definition_type(&Expr::var("unknown")), None);
}

#[test]
fn symbol_aware_expression_types_cover_index_and_numeric_copies() {
    let symbols = BTreeMap::from([
        (
            "text".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::ByteString,
            },
        ),
        (
            "buffer".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(1),
                value_type: ValueType::Buffer,
            },
        ),
        (
            "left".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(2),
                value_type: ValueType::Integer,
            },
        ),
        (
            "right".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(3),
                value_type: ValueType::Integer,
            },
        ),
    ]);

    assert_eq!(
        concrete_definition_type_with_symbols(
            &Expr::index(Expr::var("text"), Expr::int(0)),
            &symbols,
        ),
        Some("BigInteger".to_string())
    );
    assert_eq!(
        concrete_definition_type_with_symbols(
            &Expr::call(
                SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Left)),
                vec![Expr::var("buffer"), Expr::int(1)],
            ),
            &symbols,
        ),
        Some("byte[]".to_string())
    );
    assert_eq!(
        concrete_definition_type_with_symbols(
            &Expr::binary(BinOp::Add, Expr::var("left"), Expr::var("right")),
            &symbols,
        ),
        Some("BigInteger".to_string())
    );
    assert_eq!(
        concrete_definition_type_with_symbols(
            &Expr::call(
                SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Pickitem)),
                vec![Expr::var("text"), Expr::int(0)],
            ),
            &symbols,
        ),
        Some("BigInteger".to_string())
    );
    assert_eq!(
        concrete_definition_type_with_symbols(
            &Expr::call(
                SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Within)),
                vec![Expr::var("left"), Expr::int(0), Expr::int(2)],
            ),
            &symbols,
        ),
        Some("bool".to_string())
    );
}

#[test]
fn framework_native_aliases_are_accepted_for_typed_declarations() {
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
    let body = Block::from(vec![
        Stmt::assign(
            "height",
            call(
                "BEF2043140362A77C15099C7E64C12F700B665DA",
                "getTransactionHeight",
            ),
        ),
        Stmt::assign(
            "signers",
            call(
                "BEF2043140362A77C15099C7E64C12F700B665DA",
                "getTransactionSigners",
            ),
        ),
    ]);
    let symbols = BTreeMap::from([
        (
            "height".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Integer,
            },
        ),
        (
            "signers".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(1),
                value_type: ValueType::Array,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(plan.declarations["height"].csharp_type, "int");
    assert_eq!(plan.declarations["signers"].csharp_type, "Signer[]");
    assert_eq!(
        concrete_definition_type(&Expr::index(
            call(
                "BEF2043140362A77C15099C7E64C12F700B665DA",
                "getTransactionSigners",
            ),
            Expr::int(0),
        )),
        Some("Signer".to_string())
    );
}

#[test]
fn typed_array_element_types_survive_aliases_but_unknown_arrays_stay_dynamic() {
    let body = Block::from(vec![
        Stmt::assign(
            "typed_array",
            Expr::NewArray {
                length: Box::new(Expr::int(2)),
                element_type: Some(ValueType::Integer),
            },
        ),
        Stmt::assign("alias", Expr::var("typed_array")),
        Stmt::assign("item", Expr::index(Expr::var("alias"), Expr::int(0))),
        Stmt::assign(
            "unknown_array",
            Expr::NewArray {
                length: Box::new(Expr::int(2)),
                element_type: None,
            },
        ),
        Stmt::assign(
            "unknown_item",
            Expr::index(Expr::var("unknown_array"), Expr::int(0)),
        ),
    ]);
    let symbols = BTreeMap::from([
        (
            "typed_array".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Array,
            },
        ),
        (
            "alias".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Array,
            },
        ),
        (
            "item".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "unknown_array".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Array,
            },
        ),
        (
            "unknown_item".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
    ]);

    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(plan.declarations["typed_array"].csharp_type, "BigInteger[]");
    assert_eq!(plan.declarations["alias"].csharp_type, "BigInteger[]");
    assert_eq!(plan.declarations["item"].csharp_type, "BigInteger");
    assert_eq!(plan.declarations["unknown_array"].csharp_type, "object[]");
    assert_eq!(plan.declarations["unknown_item"].csharp_type, "dynamic");
}

#[test]
fn unknown_pickitem_provenance_remains_dynamic() {
    let body = Block::from(vec![Stmt::assign(
        "item",
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Pickitem)),
            vec![Expr::var("items"), Expr::int(0)],
        ),
    )]);
    let symbols = BTreeMap::from([
        (
            "items".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Array,
            },
        ),
        (
            "item".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
    ]);

    let plan = plan_declarations(&body, &symbols, true);
    assert_eq!(plan.declarations["item"].csharp_type, "dynamic");
}

#[test]
fn repeated_concrete_definitions_keep_their_shared_csharp_type() {
    let body = Block::from(vec![
        Stmt::assign("value", Expr::int(1)),
        Stmt::assign("value", Expr::int(2)),
    ]);
    let symbols = BTreeMap::from([(
        "value".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            // A join can leave the neutral SSA value type unknown even when
            // every reachable definition has the same concrete expression type.
            value_type: ValueType::Unknown,
        },
    )]);

    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(plan.declarations["value"].csharp_type, "BigInteger");
}

#[test]
fn repeated_conflicting_definitions_remain_dynamic() {
    let body = Block::from(vec![
        Stmt::assign("value", Expr::int(1)),
        Stmt::assign("value", Expr::Literal(Literal::Bool(true))),
    ]);
    let symbols = BTreeMap::from([(
        "value".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Unknown,
        },
    )]);

    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(plan.declarations["value"].csharp_type, "dynamic");
}

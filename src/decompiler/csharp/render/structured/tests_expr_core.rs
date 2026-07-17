use super::super::*;

#[test]
fn renders_all_expression_variants() {
    let context = ExprContext::default();
    let cases = vec![
        (Expr::int(42), "42"),
        (
            Expr::Literal(Literal::BigInt("18446744073709551616".to_string())),
            "BigInteger.Parse(\"18446744073709551616\")",
        ),
        (Expr::Literal(Literal::Bool(true)), "true"),
        (
            Expr::Literal(Literal::String("quote \" slash \\ tab\t nul\0".to_string())),
            "\"quote \\\" slash \\\\ tab\\t nul\\0\"",
        ),
        (
            Expr::Literal(Literal::Bytes(vec![0, 255])),
            "(ByteString)new byte[] { 0x00, 0xFF }",
        ),
        (Expr::Literal(Literal::Null), "null"),
        (Expr::Unknown, "(dynamic)null"),
        (Expr::var("@class"), "@class"),
        (Expr::index(Expr::var("items"), Expr::int(1)), "items[1]"),
        (
            Expr::Member {
                base: Box::new(Expr::var("items")),
                name: "Count".to_string(),
            },
            "items.Count",
        ),
        (
            Expr::Member {
                base: Box::new(Expr::Literal(Literal::Bytes(vec![1]))),
                name: "Length".to_string(),
            },
            "((ByteString)new byte[] { 0x01 }).Length",
        ),
        (
            Expr::Member {
                base: Box::new(Expr::Unknown),
                name: "Length".to_string(),
            },
            "((dynamic)null).Length",
        ),
        (
            Expr::Cast {
                expr: Box::new(Expr::binary(BinOp::Add, Expr::var("a"), Expr::var("b"))),
                target_type: "BigInteger".to_string(),
            },
            "(BigInteger)(a + b)",
        ),
        (
            Expr::Convert {
                value: Box::new(Expr::var("value")),
                target: ValueType::Integer,
            },
            "__NeoDecompilerConvertInteger(value)",
        ),
        (
            Expr::IsType {
                value: Box::new(Expr::var("value")),
                target: ValueType::Integer,
            },
            "__NeoDecompilerIsTypeInteger(value)",
        ),
        (
            Expr::NewArray {
                length: Box::new(Expr::int(2)),
                element_type: Some(ValueType::Integer),
            },
            "new BigInteger[(int)(2)]",
        ),
        (
            Expr::Array(vec![Expr::int(1), Expr::int(2)]),
            "new object[] { 1, 2 }",
        ),
        (
            Expr::Struct(vec![Expr::int(1), Expr::int(2)]),
            "__NeoDecompilerConvertStruct(new object[] { 1, 2 })",
        ),
        (
            Expr::Map(vec![(
                Expr::Literal(Literal::String("key".to_string())),
                Expr::int(1),
            )]),
            "new Map<object, object> { [\"key\"] = 1 }",
        ),
        (
            Expr::Ternary {
                condition: Box::new(Expr::var("condition")),
                then_expr: Box::new(Expr::int(1)),
                else_expr: Box::new(Expr::int(2)),
            },
            "condition ? 1 : 2",
        ),
        (
            Expr::Ternary {
                condition: Box::new(Expr::var("condition")),
                then_expr: Box::new(Expr::Literal(Literal::Bool(true))),
                else_expr: Box::new(Expr::var("value")),
            },
            "condition || value",
        ),
        (
            Expr::Ternary {
                condition: Box::new(Expr::var("condition")),
                then_expr: Box::new(Expr::var("value")),
                else_expr: Box::new(Expr::Literal(Literal::Bool(true))),
            },
            "!(condition) || value",
        ),
        (
            Expr::Ternary {
                condition: Box::new(Expr::var("condition")),
                then_expr: Box::new(Expr::Literal(Literal::Bool(false))),
                else_expr: Box::new(Expr::var("value")),
            },
            "!(condition) && value",
        ),
        (Expr::StackTemp(7), "_tmp7"),
    ];
    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }

    let binary_cases = [
        (BinOp::Add, "a + b"),
        (BinOp::Sub, "a - b"),
        (BinOp::Mul, "a * b"),
        (BinOp::Div, "a / b"),
        (BinOp::Mod, "a % b"),
        (BinOp::Pow, "BigInteger.Pow(a, (int)(b))"),
        (BinOp::And, "a & b"),
        (BinOp::Or, "a | b"),
        (BinOp::Xor, "a ^ b"),
        (BinOp::Shl, "a << (int)(b)"),
        (BinOp::Shr, "a >> (int)(b)"),
        (
            BinOp::Eq,
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x97 }, CallFlags.All, new object[] { a, b })",
        ),
        (
            BinOp::Ne,
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x98 }, CallFlags.All, new object[] { a, b })",
        ),
        (BinOp::Lt, "a < b"),
        (BinOp::Le, "a <= b"),
        (BinOp::Gt, "a > b"),
        (BinOp::Ge, "a >= b"),
        (
            BinOp::LogicalAnd,
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0xAB }, CallFlags.All, new object[] { a, b })",
        ),
        (
            BinOp::LogicalOr,
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0xAC }, CallFlags.All, new object[] { a, b })",
        ),
    ];
    for (operator, expected) in binary_cases {
        let expression = Expr::binary(operator, Expr::var("a"), Expr::var("b"));
        assert_eq!(render_expr(&expression, &context), expected, "{operator:?}");
    }

    let unary_cases = [
        (UnaryOp::Neg, "-value"),
        (UnaryOp::Not, "~value"),
        (UnaryOp::LogicalNot, "!(bool)(object)(value)"),
        (UnaryOp::Inc, "value + 1"),
        (UnaryOp::Dec, "value - 1"),
        (UnaryOp::Abs, "BigInteger.Abs(value)"),
        (UnaryOp::Sign, "value.Sign"),
    ];
    for (operator, expected) in unary_cases {
        let expression = Expr::unary(operator, Expr::var("value"));
        assert_eq!(render_expr(&expression, &context), expected, "{operator:?}");
    }

    let intrinsic_cases = [
        (OpCode::Within, 3, "Helper.Within(a, b, c)"),
        (
            OpCode::Substr,
            3,
            "Runtime.LoadScript((ByteString)new byte[] { 0x8C }, CallFlags.All, new object[] { a, b, c })",
        ),
        (OpCode::Modmul, 3, "Helper.ModMultiply(a, b, c)"),
        (OpCode::Modpow, 3, "BigInteger.ModPow(a, b, c)"),
        (OpCode::Sqrt, 1, "Helper.Sqrt(a)"),
        (OpCode::Nz, 1, "(BigInteger)(dynamic)(a) != 0"),
        (
            OpCode::Size,
            1,
            "(BigInteger)Runtime.LoadScript((ByteString)new byte[] { 0xCA }, CallFlags.All, new object[] { a })",
        ),
        (OpCode::Keys, 1, "a.Keys"),
        (OpCode::Values, 1, "a.Values"),
        (OpCode::Isnull, 1, "a is null"),
        (OpCode::Newbuffer, 1, "new byte[(int)(a)]"),
        (
            OpCode::Cat,
            2,
            "Runtime.LoadScript((ByteString)new byte[] { 0x8B }, CallFlags.All, new object[] { a, b })",
        ),
        (
            OpCode::Left,
            2,
            "Runtime.LoadScript((ByteString)new byte[] { 0x8D }, CallFlags.All, new object[] { a, b })",
        ),
        (
            OpCode::Right,
            2,
            "Runtime.LoadScript((ByteString)new byte[] { 0x8E }, CallFlags.All, new object[] { a, b })",
        ),
        (OpCode::Min, 2, "BigInteger.Min(a, b)"),
        (OpCode::Max, 2, "BigInteger.Max(a, b)"),
        (OpCode::Newarray0, 0, "new object[0]"),
        (OpCode::Newarray, 1, "new object[(int)(a)]"),
        (OpCode::NewarrayT, 1, "new object[(int)(a)]"),
        (OpCode::Newstruct0, 0, "new object[] { }"),
        (OpCode::Newstruct, 1, "new object[(int)(a)]"),
        (OpCode::Newmap, 0, "new Map<object, object>()"),
        (OpCode::Haskey, 2, "a.HasKey(b)"),
        (OpCode::Pickitem, 2, "((dynamic)(a))[b]"),
        (OpCode::Setitem, 3, "((dynamic)(a))[b] = c"),
        (
            OpCode::Append,
            2,
            "((Neo.SmartContract.Framework.List<object>)a).Add(b)",
        ),
        (
            OpCode::Remove,
            2,
            "Runtime.LoadScript((ByteString)new byte[] { 0xD2 }, CallFlags.All, new object[] { a, b })",
        ),
        (
            OpCode::Clearitems,
            1,
            "Runtime.LoadScript((ByteString)new byte[] { 0xD3 }, CallFlags.All, new object[] { a })",
        ),
        (OpCode::Reverseitems, 1, "Helper.Reverse(a)"),
        (
            OpCode::Popitem,
            1,
            "((Neo.SmartContract.Framework.List<object>)a).PopItem()",
        ),
        (
            OpCode::Memcpy,
            5,
            "Runtime.LoadScript((ByteString)new byte[] { 0x89 }, CallFlags.All, new object[] { a, b, c, d, e })",
        ),
        (OpCode::Convert, 1, "(object)(a)"),
        (OpCode::Istype, 1, "a is object"),
    ];
    for (opcode, argument_count, expected) in intrinsic_cases {
        let args = ["a", "b", "c", "d", "e"]
            .into_iter()
            .take(argument_count)
            .map(Expr::var)
            .collect();
        let expression = Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        );
        assert_eq!(render_expr(&expression, &context), expected, "{opcode:?}");
    }

    let user_append_call = Expr::call(
        SemanticCallTarget::Internal {
            offset: 12,
            name: "append".to_string(),
        },
        vec![Expr::var("items")],
    );
    let token_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 2,
            name: "transfer".to_string(),
            hash_le: Some("00112233445566778899AABBCCDDEEFF00112233".to_string()),
            call_flags: Some(0x0F),
        },
        vec![Expr::var("items")],
    );
    let known_syscall = Expr::call(
        SemanticCallTarget::Syscall {
            hash: 0x8CEC_27F8,
            name: Some("not trusted for dispatch".to_string()),
        },
        vec![Expr::var("items")],
    );
    let unknown_syscall = Expr::call(
        SemanticCallTarget::Syscall {
            hash: 0xDEAD_BEEF,
            name: None,
        },
        vec![Expr::var("items")],
    );
    let vm_append_call = Expr::call(
        SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Append)),
        vec![Expr::var("items"), Expr::var("value")],
    );
    assert_eq!(render_expr(&user_append_call, &context), "append(items)");
    assert_eq!(
        render_expr(&token_call, &context),
        "(dynamic)Contract.Call((UInt160)new byte[] { 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22, 0x33 }, \"transfer\", (CallFlags)0x0F, new object[] { items })"
    );
    let native_token_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 3,
            name: "getCandidates".to_string(),
            hash_le: Some("F563EA40BC283D4D0E05C48EA305B3F2A07340EF".to_string()),
            call_flags: Some(0x0F),
        },
        vec![Expr::var("items")],
    );
    assert_eq!(
        render_expr(&native_token_call, &context),
        "NeoToken.GetCandidates(items)"
    );
    let native_property_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 5,
            name: "symbol".to_string(),
            hash_le: Some("CF76E28BD0062C4A478EE35561011319F3CFA4D2".to_string()),
            call_flags: Some(0x0F),
        },
        vec![],
    );
    assert_eq!(
        render_expr(&native_property_call, &context),
        "GasToken.Symbol"
    );
    let ledger_property_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 6,
            name: "currentHash".to_string(),
            hash_le: Some("BEF2043140362A77C15099C7E64C12F700B665DA".to_string()),
            call_flags: Some(0x0F),
        },
        vec![],
    );
    assert_eq!(
        render_expr(&ledger_property_call, &context),
        "LedgerContract.CurrentHash"
    );
    let memory_search_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 7,
            name: "memorySearch".to_string(),
            hash_le: Some("C0EF39CEE0E4E925C6C2A06A79E1440DD86FCEAC".to_string()),
            call_flags: Some(0x0F),
        },
        vec![Expr::var("items"), Expr::var("value"), Expr::var("index")],
    );
    assert_eq!(
        render_expr(&memory_search_call, &context),
        "StdLib.MemorySearch(items, value, (int)(index))"
    );
    let typed_memory_context = expr_context_with_types(&[
        ("items", ValueType::ByteString),
        ("value", ValueType::Integer),
        ("index", ValueType::Integer),
    ]);
    assert_eq!(
        render_expr(&memory_search_call, &typed_memory_context),
        "StdLib.MemorySearch(items, (ByteString)(value), (int)(index))"
    );
    let exact_memory_context = ExprContext::default().with_concrete_types(&BTreeMap::from([
        ("items".to_string(), "ByteString".to_string()),
        ("value".to_string(), "ByteString".to_string()),
        ("index".to_string(), "int".to_string()),
    ]));
    assert_eq!(
        render_expr(&memory_search_call, &exact_memory_context),
        "StdLib.MemorySearch(items, value, index)"
    );
    let role_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 8,
            name: "getDesignatedByRole".to_string(),
            hash_le: Some("E295E391544C178AD94F03EC4DCDFF78534ECF49".to_string()),
            call_flags: Some(0x0F),
        },
        vec![Expr::int(8), Expr::int(0)],
    );
    assert_eq!(
        render_expr(&role_call, &context),
        "RoleManagement.GetDesignatedByRole((Role)(int)(8), 0)"
    );
    let exact_role_context = ExprContext::default().with_concrete_types(&BTreeMap::from([
        ("role".to_string(), "Role".to_string()),
        ("index".to_string(), "int".to_string()),
    ]));
    let typed_role_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 8,
            name: "getDesignatedByRole".to_string(),
            hash_le: Some("E295E391544C178AD94F03EC4DCDFF78534ECF49".to_string()),
            call_flags: Some(0x0F),
        },
        vec![Expr::var("role"), Expr::var("index")],
    );
    assert_eq!(
        render_expr(&typed_role_call, &exact_role_context),
        "RoleManagement.GetDesignatedByRole(role, index)"
    );
    let oracle_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 9,
            name: "getPrice".to_string(),
            hash_le: Some("588717117E0AA81072AFAB71D2DD89FE7C4B92FE".to_string()),
            call_flags: Some(0x0F),
        },
        vec![],
    );
    assert_eq!(
        render_expr(&oracle_call, &context),
        "OracleContract.GetPrice()"
    );
    let crypto_case_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 11,
            name: "ripemd160".to_string(),
            hash_le: Some("1BF575AB1189688413610A35A12886CDE0B66C72".to_string()),
            call_flags: Some(0x0F),
        },
        vec![Expr::var("items")],
    );
    assert_eq!(
        render_expr(&crypto_case_call, &context),
        "CryptoLib.Ripemd160(items)"
    );
    let policy_enum_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 13,
            name: "getAttributeFee".to_string(),
            hash_le: Some("7BC681C0A1F71D543457B68BBA8D5F9FDD4E5ECC".to_string()),
            call_flags: Some(0x0F),
        },
        vec![Expr::int(1)],
    );
    assert_eq!(
        render_expr(&policy_enum_call, &context),
        "PolicyContract.GetAttributeFee((TransactionAttributeType)(int)(1))"
    );
    let exact_policy_context = ExprContext::default().with_concrete_types(&BTreeMap::from([(
        "attribute_type".to_string(),
        "TransactionAttributeType".to_string(),
    )]));
    let typed_policy_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 13,
            name: "getAttributeFee".to_string(),
            hash_le: Some("7BC681C0A1F71D543457B68BBA8D5F9FDD4E5ECC".to_string()),
            call_flags: Some(0x0F),
        },
        vec![Expr::var("attribute_type")],
    );
    assert_eq!(
        render_expr(&typed_policy_call, &exact_policy_context),
        "PolicyContract.GetAttributeFee(attribute_type)"
    );
    let crypto_enum_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 14,
            name: "verifyWithECDsa".to_string(),
            hash_le: Some("1BF575AB1189688413610A35A12886CDE0B66C72".to_string()),
            call_flags: Some(0x0F),
        },
        vec![
            Expr::var("message"),
            Expr::var("pubkey"),
            Expr::var("signature"),
            Expr::int(22),
        ],
    );
    assert_eq!(
        render_expr(&crypto_enum_call, &context),
        "CryptoLib.VerifyWithECDsa(message, pubkey, signature, (NamedCurveHash)(int)(22))"
    );
    let exact_crypto_context = ExprContext::default().with_concrete_types(&BTreeMap::from([(
        "curve_hash".to_string(),
        "NamedCurveHash".to_string(),
    )]));
    let typed_crypto_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 14,
            name: "verifyWithECDsa".to_string(),
            hash_le: Some("1BF575AB1189688413610A35A12886CDE0B66C72".to_string()),
            call_flags: Some(0x0F),
        },
        vec![
            Expr::var("message"),
            Expr::var("pubkey"),
            Expr::var("signature"),
            Expr::var("curve_hash"),
        ],
    );
    assert_eq!(
        render_expr(&typed_crypto_call, &exact_crypto_context),
        "CryptoLib.VerifyWithECDsa(message, pubkey, signature, curve_hash)"
    );
    let missing_oracle_method = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 12,
            name: "finish".to_string(),
            hash_le: Some("588717117E0AA81072AFAB71D2DD89FE7C4B92FE".to_string()),
            call_flags: Some(0x0F),
        },
        vec![],
    );
    let missing_oracle_rendered = render_expr(&missing_oracle_method, &context);
    assert!(missing_oracle_rendered.contains("Contract.Call"));
    assert!(!missing_oracle_rendered.contains("OracleContract.Finish"));
    let unsupported_catalog_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 10,
            name: "getCommittee".to_string(),
            hash_le: Some("67CA70350663BF258CA513049467C6059D15E74C".to_string()),
            call_flags: Some(0x0F),
        },
        vec![],
    );
    let unsupported_rendered = render_expr(&unsupported_catalog_call, &context);
    assert!(unsupported_rendered.contains("Contract.Call"));
    assert!(!unsupported_rendered.contains("Governance.GetCommittee"));
    let restricted_native_token_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 4,
            name: "getCandidates".to_string(),
            hash_le: Some("F563EA40BC283D4D0E05C48EA305B3F2A07340EF".to_string()),
            call_flags: Some(0x01),
        },
        vec![Expr::var("items")],
    );
    assert!(render_expr(&restricted_native_token_call, &context).contains("Contract.Call"));
    assert_eq!(
        render_expr(&known_syscall, &context),
        "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x41, 0xF8, 0x27, 0xEC, 0x8C }, CallFlags.All, new object[] { items })"
    );
    assert_eq!(
        render_expr(&unknown_syscall, &context),
        "Runtime.LoadScript((ByteString)new byte[] { 0x41, 0xEF, 0xBE, 0xAD, 0xDE }, CallFlags.All, new object[] { items })"
    );
    assert_eq!(
        render_expr(&vm_append_call, &context),
        "((Neo.SmartContract.Framework.List<object>)items).Add(value)"
    );
    assert_eq!(
        render_expr(
            &Expr::unresolved_call("call_0x1234", vec![Expr::var("items")]),
            &context,
        ),
        "__NeoDecompilerUnresolvedCall(\"call_0x1234\", new object[] { items })"
    );
}

use super::*;

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

#[test]
fn collection_intrinsics_use_the_receiver_container_type() {
    let context = expr_context_with_types(&[
        ("items", ValueType::Array),
        ("map", ValueType::Map),
        ("bytes", ValueType::Buffer),
        ("text", ValueType::ByteString),
        ("index", ValueType::Integer),
        ("value", ValueType::Integer),
    ]);
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };

    let cases = [
        (
            intrinsic(OpCode::Size, vec![Expr::var("items")]),
            "items.Length",
        ),
        (intrinsic(OpCode::Size, vec![Expr::var("map")]), "map.Count"),
        (
            intrinsic(OpCode::Size, vec![Expr::var("bytes")]),
            "bytes.Length",
        ),
        (
            intrinsic(OpCode::Append, vec![Expr::var("items"), Expr::var("value")]),
            "((Neo.SmartContract.Framework.List<object>)items).Add(value)",
        ),
        (
            intrinsic(OpCode::Remove, vec![Expr::var("items"), Expr::var("index")]),
            "((Neo.SmartContract.Framework.List<object>)items).RemoveAt((int)(index))",
        ),
        (
            intrinsic(OpCode::Remove, vec![Expr::var("map"), Expr::var("key")]),
            "map.Remove(key)",
        ),
        (
            intrinsic(OpCode::Clearitems, vec![Expr::var("items")]),
            "((Neo.SmartContract.Framework.List<object>)items).Clear()",
        ),
        (
            intrinsic(OpCode::Clearitems, vec![Expr::var("map")]),
            "map.Clear()",
        ),
        (
            intrinsic(
                OpCode::Pickitem,
                vec![Expr::var("items"), Expr::var("index")],
            ),
            "items[(int)(index)]",
        ),
        (
            intrinsic(OpCode::Pickitem, vec![Expr::var("map"), Expr::var("key")]),
            "map[key]",
        ),
        (
            intrinsic(
                OpCode::Setitem,
                vec![Expr::var("bytes"), Expr::var("index"), Expr::var("value")],
            ),
            "bytes[(int)(index)] = (byte)(dynamic)(value)",
        ),
        (
            intrinsic(
                OpCode::Setitem,
                vec![Expr::var("items"), Expr::var("index"), Expr::var("value")],
            ),
            "items[(int)(index)] = value",
        ),
        (
            intrinsic(
                OpCode::Setitem,
                vec![Expr::var("text"), Expr::var("index"), Expr::var("value")],
            ),
            "((byte[])(text))[(int)(index)] = (byte)(dynamic)(value)",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn ambiguous_collection_intrinsics_use_low_level_wrappers() {
    let context = ExprContext::default();
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };
    let cases = [
        (
            intrinsic(OpCode::Size, vec![Expr::var("container")]),
            "(BigInteger)Runtime.LoadScript((ByteString)new byte[] { 0xCA }, CallFlags.All, new object[] { container })",
        ),
        (
            intrinsic(
                OpCode::Remove,
                vec![Expr::var("container"), Expr::var("key")],
            ),
            "Runtime.LoadScript((ByteString)new byte[] { 0xD2 }, CallFlags.All, new object[] { container, key })",
        ),
        (
            intrinsic(OpCode::Clearitems, vec![Expr::var("container")]),
            "Runtime.LoadScript((ByteString)new byte[] { 0xD3 }, CallFlags.All, new object[] { container })",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn indexing_intrinsics_guard_unsupported_receivers() {
    let context =
        expr_context_with_types(&[("flag", ValueType::Boolean), ("items", ValueType::Any)]);
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };
    let cases = [
        (
            intrinsic(OpCode::Pickitem, vec![Expr::var("flag"), Expr::var("key")]),
            "((dynamic)(flag))[key]",
        ),
        (
            intrinsic(OpCode::Pickitem, vec![Expr::var("items"), Expr::var("key")]),
            "((dynamic)(items))[key]",
        ),
        (
            intrinsic(OpCode::Pickitem, vec![Expr::Unknown, Expr::var("key")]),
            "((dynamic)((dynamic)null))[key]",
        ),
        (
            intrinsic(
                OpCode::Setitem,
                vec![Expr::var("flag"), Expr::var("key"), Expr::var("value")],
            ),
            "((dynamic)(flag))[key] = value",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn byte_intrinsics_use_framework_compatible_conversions() {
    let context = expr_context_with_types(&[
        ("text", ValueType::ByteString),
        ("buffer", ValueType::Buffer),
        ("destination", ValueType::Buffer),
        ("integer", ValueType::Integer),
        ("items", ValueType::Array),
    ]);
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };
    let cases = [
        (
            intrinsic(
                OpCode::Cat,
                vec![Expr::var("text"), Expr::var("buffer")],
            ),
            "Helper.Concat((ByteString)(text), (ByteString)(buffer))",
        ),
        (
            intrinsic(
                OpCode::Cat,
                vec![Expr::var("buffer"), Expr::var("text")],
            ),
            "Helper.Concat((byte[])(buffer), (ByteString)(text))",
        ),
        (
            intrinsic(
                OpCode::Cat,
                vec![Expr::var("integer"), Expr::var("items")],
            ),
            "Helper.Concat((ByteString)(dynamic)(integer), (ByteString)(dynamic)(items))",
        ),
        (
            intrinsic(
                OpCode::Substr,
                vec![Expr::var("text"), Expr::var("start"), Expr::var("length")],
            ),
            "(ByteString)(Helper.Range((byte[])(ByteString)(text), (int)(start), (int)(length)))",
        ),
        (
            intrinsic(
                OpCode::Left,
                vec![Expr::var("text"), Expr::var("count")],
            ),
            "(ByteString)(Helper.Take((byte[])(ByteString)(text), (int)(count)))",
        ),
        (
            intrinsic(
                OpCode::Right,
                vec![Expr::var("buffer"), Expr::var("count")],
            ),
            "Helper.Last((byte[])(buffer), (int)(count))",
        ),
        (
            intrinsic(
                OpCode::Memcpy,
                vec![
                    Expr::var("destination"),
                    Expr::var("destination_index"),
                    Expr::var("text"),
                    Expr::var("source_index"),
                    Expr::var("count"),
                ],
            ),
            "Array.Copy((byte[])(text), (int)(source_index), (byte[])(destination), (int)(destination_index), (int)(count))",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn value_equality_uses_csharp_operators_only_for_known_value_types() {
    let context = expr_context_with_types(&[
        ("integer_left", ValueType::Integer),
        ("integer_right", ValueType::Integer),
        ("boolean_left", ValueType::Boolean),
        ("boolean_right", ValueType::Boolean),
        ("text", ValueType::ByteString),
        ("buffer", ValueType::Buffer),
        ("array", ValueType::Array),
        ("structure", ValueType::Struct),
        ("map", ValueType::Map),
        ("unknown", ValueType::Unknown),
    ]);
    let cases = [
        (
            Expr::binary(
                BinOp::Eq,
                Expr::var("integer_left"),
                Expr::var("integer_right"),
            ),
            "integer_left == integer_right",
        ),
        (
            Expr::binary(
                BinOp::Ne,
                Expr::var("boolean_left"),
                Expr::var("boolean_right"),
            ),
            "boolean_left != boolean_right",
        ),
        (
            Expr::binary(BinOp::Eq, Expr::var("text"), Expr::var("text")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x97 }, CallFlags.All, new object[] { text, text })",
        ),
        (
            Expr::binary(BinOp::Ne, Expr::var("buffer"), Expr::var("buffer")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x98 }, CallFlags.All, new object[] { buffer, buffer })",
        ),
        (
            Expr::binary(BinOp::Eq, Expr::var("array"), Expr::var("array")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x97 }, CallFlags.All, new object[] { array, array })",
        ),
        (
            Expr::binary(
                BinOp::Ne,
                Expr::var("structure"),
                Expr::var("structure"),
            ),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x98 }, CallFlags.All, new object[] { structure, structure })",
        ),
        (
            Expr::binary(BinOp::Eq, Expr::var("map"), Expr::var("map")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x97 }, CallFlags.All, new object[] { map, map })",
        ),
        (
            Expr::binary(
                BinOp::Ne,
                Expr::Literal(Literal::Null),
                Expr::Literal(Literal::Null),
            ),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x98 }, CallFlags.All, new object[] { null, null })",
        ),
        (
            Expr::binary(BinOp::Eq, Expr::var("unknown"), Expr::var("unknown")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x97 }, CallFlags.All, new object[] { unknown, unknown })",
        ),
        (
            Expr::binary(BinOp::Ne, Expr::var("text"), Expr::var("buffer")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x98 }, CallFlags.All, new object[] { text, buffer })",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn logical_not_uses_vm_truthiness_for_integer_operands() {
    let context = expr_context_with_types(&[("number", ValueType::Integer)]);

    assert_eq!(
        render_expr(
            &Expr::unary(UnaryOp::LogicalNot, Expr::var("number")),
            &context,
        ),
        "(BigInteger)(dynamic)(number) == 0"
    );
}

#[test]
fn isnull_is_false_for_non_nullable_value_types() {
    let context =
        expr_context_with_types(&[("number", ValueType::Integer), ("flag", ValueType::Boolean)]);
    for name in ["number", "flag"] {
        let expression = Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Isnull)),
            vec![Expr::var(name)],
        );
        assert_eq!(render_expr(&expression, &context), "false");
    }
}

#[test]
fn numeric_operators_use_vm_wrappers_for_static_any_values() {
    let context = expr_context_with_types(&[
        ("left", ValueType::Any),
        ("right", ValueType::Any),
        ("integer_left", ValueType::Integer),
        ("integer_right", ValueType::Integer),
    ]);

    assert_eq!(
        render_expr(
            &Expr::binary(BinOp::Add, Expr::var("left"), Expr::var("right")),
            &context,
        ),
        "Runtime.LoadScript((ByteString)new byte[] { 0x9E }, CallFlags.All, new object[] { left, right })"
    );
    assert_eq!(
        render_expr(
            &Expr::binary(
                BinOp::Add,
                Expr::var("integer_left"),
                Expr::var("integer_right"),
            ),
            &context,
        ),
        "integer_left + integer_right"
    );
}

#[test]
fn vm_boolean_binary_operators_are_eager_only_for_known_booleans() {
    let context = expr_context_with_types(&[
        ("boolean_left", ValueType::Boolean),
        ("boolean_right", ValueType::Boolean),
        ("integer_left", ValueType::Integer),
        ("integer_right", ValueType::Integer),
        ("unknown", ValueType::Unknown),
    ]);
    let cases = [
        (
            Expr::binary(
                BinOp::LogicalAnd,
                Expr::var("boolean_left"),
                Expr::var("boolean_right"),
            ),
            "boolean_left & boolean_right",
        ),
        (
            Expr::binary(
                BinOp::LogicalOr,
                Expr::var("boolean_left"),
                Expr::var("boolean_right"),
            ),
            "boolean_left | boolean_right",
        ),
        (
            Expr::binary(
                BinOp::LogicalAnd,
                Expr::var("unknown"),
                Expr::var("unknown"),
            ),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0xAB }, CallFlags.All, new object[] { unknown, unknown })",
        ),
        (
            Expr::binary(
                BinOp::LogicalOr,
                Expr::var("integer_left"),
                Expr::var("integer_right"),
            ),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0xAC }, CallFlags.All, new object[] { integer_left, integer_right })",
        ),
        (
            Expr::binary(
                BinOp::LogicalAnd,
                Expr::var("boolean_left"),
                Expr::var("unknown"),
            ),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0xAB }, CallFlags.All, new object[] { boolean_left, unknown })",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn syscall_rendering_uses_hash_identity_and_drops_display_metadata() {
    let context = ExprContext::default();
    let cases = [
        (
            Expr::call(
                SemanticCallTarget::Syscall {
                    hash: 0x8CEC_27F8,
                    name: Some("System.Runtime.CheckWitness".to_string()),
                },
                vec![
                    Expr::Literal(Literal::String(
                        "System.Runtime.CheckWitness".to_string(),
                    )),
                    Expr::var("account"),
                ],
            ),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x41, 0xF8, 0x27, 0xEC, 0x8C }, CallFlags.All, new object[] { account })",
        ),
        (
            Expr::call(
                SemanticCallTarget::Syscall {
                    hash: 0x0388_C3B7,
                    name: Some("ignored".to_string()),
                },
                vec![Expr::Literal(Literal::String(
                    "System.Runtime.GetTime".to_string(),
                ))],
            ),
            "Runtime.Time",
        ),
        (
            Expr::call(
                SemanticCallTarget::Syscall {
                    hash: 0xCE67_F69B,
                    name: None,
                },
                vec![Expr::Literal(Literal::String(
                    "System.Storage.GetContext".to_string(),
                ))],
            ),
            "Storage.CurrentContext",
        ),
        (
            Expr::call(
                SemanticCallTarget::Syscall {
                    hash: 0x9CED_089C,
                    name: None,
                },
                vec![
                    Expr::Literal(Literal::String("System.Iterator.Next".to_string())),
                    Expr::var("iterator"),
                ],
            ),
            "((Iterator)iterator).Next()",
        ),
        (
            Expr::call(
                SemanticCallTarget::Syscall {
                    hash: 0x616F_0195,
                    name: Some("System.Runtime.Notify".to_string()),
                },
                vec![
                    Expr::Literal(Literal::String("System.Runtime.Notify".to_string())),
                    Expr::var("event_name"),
                    Expr::var("state"),
                ],
            ),
            "Runtime.LoadScript((ByteString)new byte[] { 0x41, 0x95, 0x01, 0x6F, 0x61 }, CallFlags.All, new object[] { event_name, state })",
        ),
        (
            Expr::call(
                SemanticCallTarget::Syscall {
                    hash: 0xDEAD_BEEF,
                    name: None,
                },
                vec![Expr::Literal(Literal::String("0xDEADBEEF".to_string()))],
            ),
            "Runtime.LoadScript((ByteString)new byte[] { 0x41, 0xEF, 0xBE, 0xAD, 0xDE }, CallFlags.All, new object[] {  })",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn compiler_debug_notify_lowers_only_proven_singleton_string_states() {
    let debug_call = |state| {
        Expr::call(
            SemanticCallTarget::Syscall {
                hash: 0x616F_0195,
                name: Some("System.Runtime.Notify".to_string()),
            },
            vec![Expr::Literal(Literal::String("Debug".to_string())), state],
        )
    };
    let direct = debug_call(Expr::Array(vec![Expr::Literal(Literal::String(
        "message".to_string(),
    ))]));
    assert_eq!(
        render_expr(&direct, &ExprContext::default()),
        "Runtime.Debug(\"message\")"
    );

    let body = Block::from(vec![
        Stmt::assign(
            "state",
            Expr::Array(vec![Expr::Literal(Literal::String(
                "aliased message".to_string(),
            ))]),
        ),
        Stmt::expr(debug_call(Expr::var("state"))),
    ]);
    let symbols = BTreeMap::from([(
        "state".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Array,
        },
    )]);
    let context = ExprContext::for_block(&body, &symbols, true);
    assert!(context.is_debug_singleton_array_target("state"));
    assert_eq!(
        render_expr(&debug_call(Expr::var("state")), &context),
        "Runtime.Debug(\"aliased message\")"
    );

    let multi_state = debug_call(Expr::Array(vec![
        Expr::Literal(Literal::String("first".to_string())),
        Expr::Literal(Literal::String("second".to_string())),
    ]));
    assert!(render_expr(&multi_state, &ExprContext::default()).starts_with("Runtime.LoadScript("));
}

#[test]
fn manifest_event_notify_lifts_only_an_exact_packed_state() {
    let notify = |state| {
        Expr::call(
            SemanticCallTarget::Syscall {
                hash: 0x616F_0195,
                name: Some("System.Runtime.Notify".to_string()),
            },
            vec![
                Expr::Literal(Literal::String("transfer".to_string())),
                state,
            ],
        )
    };
    let body = Block::from(vec![
        Stmt::assign(
            "state",
            Expr::Array(vec![
                Expr::var("from"),
                Expr::var("to"),
                Expr::var("amount"),
            ]),
        ),
        Stmt::expr(notify(Expr::var("state"))),
    ]);
    let symbols = BTreeMap::from([
        (
            "state".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Array,
            },
        ),
        (
            "from".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Buffer,
            },
        ),
        (
            "to".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Buffer,
            },
        ),
        (
            "amount".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
    ]);
    let signatures = BTreeMap::from([(
        "transfer".to_string(),
        (
            "transfer".to_string(),
            vec![
                "ByteString".to_string(),
                "ByteString".to_string(),
                "BigInteger".to_string(),
            ],
        ),
    )]);
    let context = ExprContext::for_block(&body, &symbols, true).with_event_signatures(&signatures);

    assert!(context.is_event_array_target("state"));
    assert_eq!(
        render_expr(&notify(Expr::var("state")), &context),
        "transfer((ByteString)(from), (ByteString)(to), amount)"
    );

    let wrong_arity = notify(Expr::Array(vec![Expr::var("from"), Expr::var("amount")]));
    assert_eq!(
        render_expr(&wrong_arity, &context),
        "Runtime.LoadScript((ByteString)new byte[] { 0x41, 0x95, 0x01, 0x6F, 0x61 }, CallFlags.All, new object[] { \"transfer\", new object[] { from, amount } })"
    );
}

#[test]
fn typed_syscall_fallbacks_preserve_catalog_return_types() {
    let context = expr_context_with_types(&[("storage", ValueType::InteropInterface)]);
    let expression = Expr::call(
        SemanticCallTarget::Syscall {
            hash: 0x31E8_5D92,
            name: Some("System.Storage.Get".to_string()),
        },
        vec![Expr::var("storage"), Expr::var("dynamic_key")],
    );

    assert_eq!(
        render_expr(&expression, &context),
        "(ByteString)Runtime.LoadScript((ByteString)new byte[] { 0x41, 0x92, 0x5D, 0xE8, 0x31 }, CallFlags.All, new object[] { storage, dynamic_key })"
    );
}

#[test]
fn check_witness_requires_explicit_framework_overload_evidence() {
    let context = expr_context_with_types(&[
        ("account_bytes", ValueType::ByteString),
        ("unknown_account", ValueType::Unknown),
    ]);
    let check_witness = |argument| {
        Expr::call(
            SemanticCallTarget::Syscall {
                hash: 0x8CEC_27F8,
                name: Some("System.Runtime.CheckWitness".to_string()),
            },
            vec![argument],
        )
    };
    let cases = [
        (
            check_witness(Expr::var("account_bytes")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x41, 0xF8, 0x27, 0xEC, 0x8C }, CallFlags.All, new object[] { account_bytes })",
        ),
        (
            check_witness(Expr::var("unknown_account")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x41, 0xF8, 0x27, 0xEC, 0x8C }, CallFlags.All, new object[] { unknown_account })",
        ),
        (
            check_witness(Expr::Cast {
                expr: Box::new(Expr::var("account_bytes")),
                target_type: "UInt160".to_string(),
            }),
            "Runtime.CheckWitness((UInt160)(account_bytes))",
        ),
        (
            check_witness(Expr::Cast {
                expr: Box::new(Expr::var("account_bytes")),
                target_type: "ECPoint".to_string(),
            }),
            "Runtime.CheckWitness((ECPoint)(account_bytes))",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn check_witness_uses_proven_address_types_without_redundant_casts() {
    let context = ExprContext::default().with_concrete_types(&BTreeMap::from([
        ("account".to_string(), "UInt160".to_string()),
        ("group".to_string(), "ECPoint".to_string()),
    ]));
    let check_witness = |argument| {
        Expr::call(
            SemanticCallTarget::Syscall {
                hash: 0x8CEC_27F8,
                name: Some("System.Runtime.CheckWitness".to_string()),
            },
            vec![argument],
        )
    };

    assert_eq!(
        render_expr(&check_witness(Expr::var("account")), &context),
        "Runtime.CheckWitness(account)"
    );
    assert_eq!(
        render_expr(&check_witness(Expr::var("group")), &context),
        "Runtime.CheckWitness(group)"
    );
}

#[test]
fn syscall_arguments_match_framework_signatures() {
    let context = ExprContext::default();
    let syscall = |hash, name: &str, args: Vec<Expr>| {
        let mut with_metadata = vec![Expr::Literal(Literal::String(name.to_string()))];
        with_metadata.extend(args);
        Expr::call(
            SemanticCallTarget::Syscall {
                hash,
                name: Some(name.to_string()),
            },
            with_metadata,
        )
    };
    let cases = [
        (
            syscall(
                0x0287_99CF,
                "System.Contract.CreateStandardAccount",
                vec![Expr::var("pubkey")],
            ),
            "Contract.CreateStandardAccount((ECPoint)(pubkey))",
        ),
        (
            syscall(
                0x09E9_336A,
                "System.Contract.CreateMultisigAccount",
                vec![Expr::var("m"), Expr::var("pubkeys")],
            ),
            "Contract.CreateMultisigAccount((int)(m), (ECPoint[])(pubkeys))",
        ),
        (
            syscall(
                0x27B3_E756,
                "System.Crypto.CheckSig",
                vec![Expr::var("pubkey"), Expr::var("signature")],
            ),
            "Crypto.CheckSig((ECPoint)(pubkey), (ByteString)(signature))",
        ),
        (
            syscall(
                0x3ADC_D09E,
                "System.Crypto.CheckMultisig",
                vec![Expr::var("pubkeys"), Expr::var("signatures")],
            ),
            "Crypto.CheckMultisig((ECPoint[])(pubkeys), (ByteString[])(signatures))",
        ),
        (
            syscall(
                0x525B_7D62,
                "System.Contract.Call",
                vec![
                    Expr::var("script_hash"),
                    Expr::var("method"),
                    Expr::var("flags"),
                    Expr::var("arguments"),
                ],
            ),
            "Contract.Call((UInt160)(script_hash), (string)(method), (CallFlags)(int)(flags), (object[])(arguments))",
        ),
        (
            syscall(
                0xBC8C_5AC3,
                "System.Runtime.BurnGas",
                vec![Expr::var("amount")],
            ),
            "Runtime.BurnGas((long)(BigInteger)(amount))",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn syscall_metadata_is_removed_only_from_the_extra_selector_slot() {
    let expression = Expr::call(
        SemanticCallTarget::Syscall {
            hash: 0x9647_E7CF,
            name: Some("System.Runtime.Log".to_string()),
        },
        vec![Expr::Literal(Literal::String(
            "System.Runtime.Log".to_string(),
        ))],
    );

    assert_eq!(
        render_expr(&expression, &ExprContext::default()),
        "Runtime.Log((string)(\"System.Runtime.Log\"))"
    );
}

#[test]
fn storage_syscalls_select_overloads_from_neutral_types() {
    let context = expr_context_with_types(&[
        ("storage", ValueType::InteropInterface),
        ("key", ValueType::Buffer),
        ("value", ValueType::Integer),
    ]);
    let expression = Expr::call(
        SemanticCallTarget::Syscall {
            hash: 0x8418_3FE6,
            name: Some("System.Storage.Put".to_string()),
        },
        vec![
            Expr::Literal(Literal::String("System.Storage.Put".to_string())),
            Expr::var("storage"),
            Expr::var("key"),
            Expr::var("value"),
        ],
    );

    assert_eq!(
        render_expr(&expression, &context),
        "Storage.Put((StorageContext)(storage), (byte[])(key), (BigInteger)(value))"
    );
}

#[test]
fn storage_syscalls_use_validated_csharp_types_when_vm_types_are_unknown() {
    let context = ExprContext::default().with_concrete_types(&BTreeMap::from([
        ("storage".to_string(), "StorageContext".to_string()),
        ("key".to_string(), "ByteString".to_string()),
        ("value".to_string(), "BigInteger".to_string()),
    ]));
    let expression = Expr::call(
        SemanticCallTarget::Syscall {
            hash: 0x8418_3FE6,
            name: Some("System.Storage.Put".to_string()),
        },
        vec![
            Expr::Literal(Literal::String("System.Storage.Put".to_string())),
            Expr::var("storage"),
            Expr::var("key"),
            Expr::var("value"),
        ],
    );

    assert_eq!(
        render_expr(&expression, &context),
        "Storage.Put(storage, key, value)"
    );
}

#[test]
fn every_known_syscall_has_an_explicit_csharp_policy() {
    for syscall in crate::syscalls::all() {
        assert!(
            known_syscall_is_classified(syscall.hash),
            "missing C# syscall policy for {} (0x{:08X})",
            syscall.name,
            syscall.hash
        );
    }
}

#[test]
fn map_intrinsics_guard_known_non_map_receivers() {
    let context = expr_context_with_types(&[
        ("flag", ValueType::Boolean),
        ("items", ValueType::Array),
        ("map", ValueType::Map),
    ]);
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };

    let keys = render_expr(&intrinsic(OpCode::Keys, vec![Expr::var("flag")]), &context);
    assert!(keys.contains("Runtime.LoadScript"), "{keys}");
    assert!(!keys.contains(".Keys"), "{keys}");

    let values = render_expr(
        &intrinsic(OpCode::Values, vec![Expr::var("items")]),
        &context,
    );
    assert!(values.contains("Runtime.LoadScript"), "{values}");
    assert!(!values.contains(".Values"), "{values}");

    let has_key = render_expr(
        &intrinsic(OpCode::Haskey, vec![Expr::var("flag"), Expr::var("map")]),
        &context,
    );
    assert!(has_key.contains("Runtime.LoadScript"), "{has_key}");
    assert!(!has_key.contains(".HasKey"), "{has_key}");

    let append = render_expr(
        &intrinsic(OpCode::Append, vec![Expr::var("map"), Expr::var("flag")]),
        &context,
    );
    assert!(append.contains("Runtime.LoadScript"), "{append}");
    assert!(!append.contains("List<object>"), "{append}");

    let popitem = render_expr(
        &intrinsic(OpCode::Popitem, vec![Expr::var("map")]),
        &context,
    );
    assert!(popitem.contains("Runtime.LoadScript"), "{popitem}");
    assert!(!popitem.contains("List<object>"), "{popitem}");

    let reverse = render_expr(
        &intrinsic(OpCode::Reverseitems, vec![Expr::var("map")]),
        &context,
    );
    assert!(reverse.contains("Runtime.LoadScript"), "{reverse}");
    assert!(!reverse.contains("Helper.Reverse"), "{reverse}");
}

#[test]
fn unmodeled_intrinsic_uses_a_low_level_wrapper() {
    let expression = Expr::call(
        SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Ldloc0)),
        vec![],
    );

    assert_eq!(
        render_expr(&expression, &ExprContext::default()),
        "Runtime.LoadScript((ByteString)new byte[] { 0x68 }, CallFlags.All, new object[] {  })"
    );
}

#[test]
fn renders_expression_precedence_from_structure() {
    let context = ExprContext::default();
    let cases = [
        (
            Expr::binary(
                BinOp::Mul,
                Expr::binary(BinOp::Add, Expr::var("a"), Expr::var("b")),
                Expr::var("c"),
            ),
            "(a + b) * c",
        ),
        (
            Expr::binary(
                BinOp::Add,
                Expr::var("a"),
                Expr::binary(BinOp::Mul, Expr::var("b"), Expr::var("c")),
            ),
            "a + b * c",
        ),
        (
            Expr::binary(
                BinOp::Sub,
                Expr::var("a"),
                Expr::binary(BinOp::Sub, Expr::var("b"), Expr::var("c")),
            ),
            "a - (b - c)",
        ),
        (
            Expr::unary(
                UnaryOp::LogicalNot,
                Expr::binary(BinOp::LogicalAnd, Expr::var("a"), Expr::var("b")),
            ),
            "!((bool)Runtime.LoadScript((ByteString)new byte[] { 0xAB }, CallFlags.All, new object[] { a, b }))",
        ),
        (
            Expr::unary(UnaryOp::Neg, Expr::unary(UnaryOp::Neg, Expr::var("value"))),
            "-(-value)",
        ),
        (
            Expr::binary(
                BinOp::Add,
                Expr::var("a"),
                Expr::Ternary {
                    condition: Box::new(Expr::var("condition")),
                    then_expr: Box::new(Expr::var("b")),
                    else_expr: Box::new(Expr::var("c")),
                },
            ),
            "a + (condition ? b : c)",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn nested_predicate_calls_parenthesize_ternary_operands() {
    let context = ExprContext::default();
    let cases = [
        (
            OpCode::Nz,
            "__NeoDecompilerUnresolvedCall(\"consume\", new object[] { (BigInteger)(dynamic)(condition ? left : right) != 0 })",
        ),
        (
            OpCode::Isnull,
            "__NeoDecompilerUnresolvedCall(\"consume\", new object[] { (condition ? left : right) is null })",
        ),
        (
            OpCode::Istype,
            "__NeoDecompilerUnresolvedCall(\"consume\", new object[] { (condition ? left : right) is object })",
        ),
    ];

    for (opcode, expected) in cases {
        let predicate = Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            vec![Expr::Ternary {
                condition: Box::new(Expr::var("condition")),
                then_expr: Box::new(Expr::var("left")),
                else_expr: Box::new(Expr::var("right")),
            }],
        );
        let nested = Expr::unresolved_call("consume", vec![predicate]);

        assert_eq!(render_expr(&nested, &context), expected, "{opcode:?}");
    }
}

#[test]
fn negative_integer_literal_does_not_form_a_decrement_token() {
    let expression = Expr::unary(UnaryOp::Neg, Expr::int(-1));

    assert_eq!(render_expr(&expression, &ExprContext::default()), "-(-1)");
}

#[test]
fn csharp_strings_escape_unicode_line_separators() {
    let expression = Expr::Literal(Literal::String(
        "before\u{2028}middle\u{2029}after".to_string(),
    ));

    assert_eq!(
        render_expr(&expression, &ExprContext::default()),
        "\"before\\u2028middle\\u2029after\""
    );
}

#[test]
fn typed_shift_counts_render_as_int() {
    let context = ExprContext::default();

    assert_eq!(
        render_expr(
            &Expr::binary(BinOp::Shl, Expr::var("value"), Expr::var("count")),
            &context,
        ),
        "value << (int)(count)"
    );
    assert_eq!(
        render_expr(
            &Expr::binary(BinOp::Shr, Expr::var("value"), Expr::var("count")),
            &context,
        ),
        "value >> (int)(count)"
    );
}

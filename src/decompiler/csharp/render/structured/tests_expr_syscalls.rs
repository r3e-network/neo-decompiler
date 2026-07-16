use super::super::*;

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

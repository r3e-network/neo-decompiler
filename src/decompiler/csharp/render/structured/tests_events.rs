use super::*;

#[test]
fn manifest_event_notify_omits_only_an_exact_packed_state_temp() {
    let body = Block::from(vec![
        Stmt::assign(
            "state",
            Expr::Array(vec![
                Expr::var("from"),
                Expr::var("to"),
                Expr::var("amount"),
            ]),
        ),
        Stmt::expr(Expr::call(
            SemanticCallTarget::Syscall {
                hash: 0x616F_0195,
                name: Some("System.Runtime.Notify".to_string()),
            },
            vec![
                Expr::Literal(Literal::String("transfer".to_string())),
                Expr::var("state"),
            ],
        )),
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
    let plan = plan_declarations(&body, &symbols, true);
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

    assert_eq!(
        super::super::stmt::render_block_with_trace(
            &body,
            &plan,
            &symbols,
            ReturnBehavior::Void,
            true,
            "__assert",
            "__NeoDecompilerVmException",
            None,
            None,
            &BTreeMap::new(),
            &BTreeMap::new(),
            Some("void"),
            None,
            &[],
            &signatures,
        ),
        "transfer((ByteString)(from), (ByteString)(to), amount);"
    );

    let wrong_arity_body = Block::from(vec![
        Stmt::assign(
            "state",
            Expr::Array(vec![Expr::var("from"), Expr::var("amount")]),
        ),
        Stmt::expr(Expr::call(
            SemanticCallTarget::Syscall {
                hash: 0x616F_0195,
                name: Some("System.Runtime.Notify".to_string()),
            },
            vec![
                Expr::Literal(Literal::String("transfer".to_string())),
                Expr::var("state"),
            ],
        )),
    ]);
    let wrong_plan = plan_declarations(&wrong_arity_body, &symbols, true);
    assert_eq!(
        super::super::stmt::render_block_with_trace(
            &wrong_arity_body,
            &wrong_plan,
            &symbols,
            ReturnBehavior::Void,
            true,
            "__assert",
            "__NeoDecompilerVmException",
            None,
            None,
            &BTreeMap::new(),
            &BTreeMap::new(),
            Some("void"),
            None,
            &[],
            &signatures,
        ),
        "object[] state = new object[] { from, amount };\nRuntime.LoadScript((ByteString)new byte[] { 0x41, 0x95, 0x01, 0x6F, 0x61 }, CallFlags.All, new object[] { \"transfer\", state });"
    );
}

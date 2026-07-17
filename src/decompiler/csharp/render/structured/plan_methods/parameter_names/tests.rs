use super::*;
use crate::decompiler::ir::UnaryOp;

#[test]
fn hints_through_wrappers() {
    assert_eq!(name_hint(&Expr::var("amount")), Some("amount".to_string()));
    assert_eq!(
        name_hint(&Expr::unary(UnaryOp::Neg, Expr::var("amount"))),
        Some("amount".to_string())
    );
    assert_eq!(
        name_hint(&Expr::Cast {
            expr: Box::new(Expr::var("owner")),
            target_type: "UInt160".to_string(),
        }),
        Some("owner".to_string())
    );
}

#[test]
fn rejects_placeholders_and_expressions() {
    assert_eq!(name_hint(&Expr::var("arg0")), None);
    assert_eq!(name_hint(&Expr::var("loc2")), None);
    assert_eq!(name_hint(&Expr::var("t_7")), None);
    assert_eq!(name_hint(&Expr::var("p4_0")), None);
    assert_eq!(name_hint(&Expr::var("static1")), None);
    assert_eq!(name_hint(&Expr::int(3)), None);
    assert_eq!(
        name_hint(&Expr::binary(
            crate::decompiler::ir::BinOp::Add,
            Expr::var("a"),
            Expr::int(1),
        )),
        None
    );
}

#[test]
fn votes_conflict_on_disagreement() {
    let mut vote = NameVote::default();
    vote.offer(Some("from".to_string()));
    vote.offer(Some("to".to_string()));
    assert_eq!(vote.unanimous(), None);

    let mut vote = NameVote::default();
    vote.offer(Some("amount".to_string()));
    vote.offer(Some("amount".to_string()));
    assert_eq!(vote.unanimous(), Some("amount"));
}

#[test]
fn call_arity_disagreement_blocks_name_inference() {
    let expected_arities = BTreeSet::from([1]);
    let observed_arities = BTreeSet::from([1, 2]);
    assert!(!recovered_call_shapes_match(
        2,
        &expected_arities,
        2,
        Some(&observed_arities),
    ));
    assert!(recovered_call_shapes_match(
        2,
        &expected_arities,
        2,
        Some(&expected_arities),
    ));
}

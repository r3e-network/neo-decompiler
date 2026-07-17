//! Readability-oriented temporary reduction over structured IR.

mod arrays;
mod casts;
mod copies;
mod dead_stores;
mod merges;
mod queries;
mod support;

use crate::decompiler::ir::Block as IrBlock;
use support::UseCounts;

/// Run the temporary-reduction passes to a fixed point.
pub(crate) fn reduce_temporaries(block: &mut IrBlock) {
    // Bounded fixpoint: each pass can unlock the next (propagation creates
    // dead stores, dead-store removal creates single-use copies).
    for _ in 0..8 {
        let before = block.clone();
        copies::propagate_single_use_copies(block);
        merges::fold_branch_value_merges(block);
        casts::simplify_casts(block);
        arrays::fold_array_initializers(block);
        dead_stores::eliminate_dead_stores(block, &UseCounts::of(block));
        if *block == before {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::ir::{BinOp, ControlFlow, Expr, Literal, SemanticCallTarget, Stmt};

    fn v(name: &str) -> Expr {
        Expr::var(name)
    }

    fn assign(name: &str, value: Expr) -> Stmt {
        Stmt::assign(name, value)
    }

    fn render(block: &IrBlock) -> String {
        crate::decompiler::ir::render_block(block, 0)
    }

    #[test]
    fn propagates_single_use_copy_into_next_statement() {
        let mut block = IrBlock::with_stmts(vec![
            assign(
                "t_3",
                Expr::Member {
                    base: Box::new(v("owner")),
                    name: "Length".to_string(),
                },
            ),
            assign("p3_0", Expr::binary(BinOp::Eq, v("t_3"), Expr::int(20))),
            Stmt::Return(Some(v("p3_0"))),
        ]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(!rendered.contains("t_3"), "{rendered}");
        assert!(!rendered.contains("p3_0"), "{rendered}");
        assert!(
            rendered.contains("return (owner.Length == 20);"),
            "{rendered}"
        );
    }

    #[test]
    fn propagates_copy_chain_through_multiple_steps() {
        let mut block = IrBlock::with_stmts(vec![
            assign("t_14", Expr::binary(BinOp::Add, v("loc1"), v("arg1"))),
            assign("loc1", v("t_14")),
            Stmt::Return(Some(v("loc1"))),
        ]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(!rendered.contains("t_14"), "{rendered}");
        assert!(rendered.contains("loc1 = (loc1 + arg1);"), "{rendered}");
    }

    #[test]
    fn keeps_multi_use_temporaries() {
        let mut block = IrBlock::with_stmts(vec![
            assign("t_2", Expr::int(7)),
            assign("a", Expr::binary(BinOp::Add, v("t_2"), Expr::int(1))),
            assign("b", Expr::binary(BinOp::Mul, v("t_2"), Expr::int(2))),
            Stmt::ExprStmt(Expr::unresolved_call("sinks", vec![v("a"), v("b")])),
        ]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(rendered.contains("t_2 = 7;"), "{rendered}");
    }

    #[test]
    fn does_not_propagate_across_interfering_assignment() {
        let mut block = IrBlock::with_stmts(vec![
            assign("t_1", Expr::binary(BinOp::Add, v("x"), Expr::int(1))),
            assign("x", Expr::int(9)),
            Stmt::Return(Some(v("t_1"))),
        ]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(rendered.contains("t_1 = (x + 1);"), "{rendered}");
        assert!(rendered.contains("return t_1;"), "{rendered}");
    }

    #[test]
    fn propagates_into_nested_branch_of_following_statement() {
        let mut block = IrBlock::with_stmts(vec![
            assign("t_5", Expr::binary(BinOp::Add, v("a"), Expr::int(1))),
            Stmt::ControlFlow(Box::new(ControlFlow::if_then(
                v("cond"),
                IrBlock::with_stmts(vec![Stmt::Return(Some(v("t_5")))]),
            ))),
        ]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(!rendered.contains("t_5"), "{rendered}");
        assert!(rendered.contains("return (a + 1);"), "{rendered}");
    }

    #[test]
    fn skips_propagation_into_loop_that_reassigns_free_variable() {
        let mut block = IrBlock::with_stmts(vec![
            assign("t_5", Expr::binary(BinOp::Add, v("a"), Expr::int(1))),
            Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
                v("cond"),
                IrBlock::with_stmts(vec![
                    assign("a", Expr::int(0)),
                    Stmt::ExprStmt(Expr::unresolved_call("sinks", vec![v("t_5")])),
                ]),
            ))),
            Stmt::Return(Some(v("a"))),
        ]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(rendered.contains("t_5 = (a + 1);"), "{rendered}");
    }

    #[test]
    fn collapses_nested_dynamic_and_identity_casts() {
        let mut block = IrBlock::with_stmts(vec![Stmt::Return(Some(Expr::Cast {
            expr: Box::new(Expr::Cast {
                expr: Box::new(v("p2_0")),
                target_type: "dynamic".to_string(),
            }),
            target_type: "BigInteger".to_string(),
        }))]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(
            rendered.contains("return (BigInteger)(p2_0);"),
            "{rendered}"
        );
        assert!(!rendered.contains("dynamic"), "{rendered}");
    }

    #[test]
    fn drops_casts_of_literals_to_natural_types() {
        let mut block = IrBlock::with_stmts(vec![Stmt::Return(Some(Expr::Cast {
            expr: Box::new(Expr::int(1)),
            target_type: "int".to_string(),
        }))]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(rendered.contains("return 1;"), "{rendered}");
    }

    #[test]
    fn removes_dead_store_and_its_now_unused_source() {
        let mut block = IrBlock::with_stmts(vec![
            assign("t_9", Expr::int(1)),
            assign("t_13", Expr::Array(vec![v("t_9")])),
            assign("loc0", v("t_13")),
            Stmt::Return(Some(v("t_9"))),
        ]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(!rendered.contains("t_13"), "{rendered}");
        assert!(!rendered.contains("loc0"), "{rendered}");
        assert!(rendered.contains("return 1;"), "{rendered}");
    }

    #[test]
    fn keeps_side_effecting_rhs_as_expression_statement() {
        let call = Expr::unresolved_call("Storage.Get", vec![v("key")]);
        let mut block = IrBlock::with_stmts(vec![assign("loc0", call)]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(rendered.contains("Storage.Get(key);"), "{rendered}");
        assert!(!rendered.contains("loc0"), "{rendered}");
    }

    #[test]
    fn keeps_dead_division_that_can_fault() {
        let mut block = IrBlock::with_stmts(vec![assign(
            "loc0",
            Expr::binary(BinOp::Div, Expr::int(1), Expr::int(0)),
        )]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(rendered.contains("loc0 = (1 / 0);"), "{rendered}");
    }

    #[test]
    fn keeps_dead_index_read_that_can_fault() {
        let mut block = IrBlock::with_stmts(vec![assign(
            "loc0",
            Expr::index(Expr::var("items"), Expr::int(0)),
        )]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(rendered.contains("loc0 = items[0];"), "{rendered}");
    }

    #[test]
    fn static_stores_are_never_dead() {
        let mut block = IrBlock::with_stmts(vec![assign("static0", Expr::int(42))]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(rendered.contains("static0 = 42;"), "{rendered}");
    }

    #[test]
    fn phi_branch_merge_folds_into_conditional_expression() {
        let mut block = IrBlock::with_stmts(vec![
            Stmt::ControlFlow(Box::new(ControlFlow::if_else(
                v("cond"),
                IrBlock::with_stmts(vec![assign("p3_0", Expr::int(0))]),
                IrBlock::with_stmts(vec![assign("p3_0", Expr::int(1))]),
            ))),
            Stmt::Return(Some(v("p3_0"))),
        ]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(rendered.contains("return (cond ? 0 : 1);"), "{rendered}");
        assert!(!rendered.contains("p3_0"), "{rendered}");
    }

    #[test]
    fn bool_false_arm_folds_into_logical_and() {
        let mut block = IrBlock::with_stmts(vec![
            Stmt::ControlFlow(Box::new(ControlFlow::if_else(
                v("is_type"),
                IrBlock::with_stmts(vec![assign(
                    "p3_0",
                    Expr::binary(BinOp::Eq, v("length"), Expr::int(20)),
                )]),
                IrBlock::with_stmts(vec![assign("p3_0", Expr::Literal(Literal::Bool(false)))]),
            ))),
            Stmt::ControlFlow(Box::new(ControlFlow::if_then(
                v("p3_0"),
                IrBlock::with_stmts(vec![Stmt::Return(Some(Expr::int(1)))]),
            ))),
        ]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        // The IR keeps the short-circuit-safe ternary; the C# renderer
        // prettifies it to `is_type && (length == 20)`.
        assert!(
            rendered.contains("if ((is_type ? (length == 20) : false))"),
            "{rendered}"
        );
        assert!(!rendered.contains("p3_0"), "{rendered}");
    }

    #[test]
    fn call_results_are_not_propagated() {
        let mut block = IrBlock::with_stmts(vec![
            assign(
                "t_5",
                Expr::Call {
                    target: SemanticCallTarget::Unresolved {
                        display_name: "Runtime.CheckWitness".to_string(),
                    },
                    args: vec![v("owner")],
                },
            ),
            Stmt::Return(Some(v("t_5"))),
        ]);
        reduce_temporaries(&mut block);

        let rendered = render(&block);
        assert!(
            rendered.contains("t_5 = Runtime.CheckWitness(owner);"),
            "{rendered}"
        );
    }
}

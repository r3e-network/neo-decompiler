
use super::*;
use crate::decompiler::cfg::ssa::{DominanceInfo, SsaBlock, SsaExpr, SsaStmt, SsaVariable};
use crate::decompiler::cfg::{BlockId, Cfg};
use crate::decompiler::ir::{BinOp, Literal};

fn v(base: &str, ver: usize) -> SsaVariable {
    SsaVariable::new(base.to_string(), ver)
}

fn assign_str(target: SsaVariable, value: SsaExpr) -> SsaStmt {
    SsaStmt::assign(target, value)
}

/// A named slot's constant value must NOT be substituted into its uses: a
/// decompiler preserves the user-visible variable reference (`loc0 < 3`)
/// rather than erasing it (`0 < 3` → `true`), which would dissolve the
/// branch condition the structurer needs. Temps (`b0`) still fold.
#[test]
fn does_not_propagate_constant_through_a_slot_variable() {
    let mut block = SsaBlock::new();
    // loc0_0 = 0  (a named local slot, not an anonymous temp)
    block.add_stmt(assign_str(v("loc0", 0), SsaExpr::lit(Literal::Int(0))));
    // t_0 = (loc0_0 < 3)  — must stay referencing loc0_0, not fold to `true`.
    block.add_stmt(assign_str(
        v("t", 0),
        SsaExpr::binary(
            BinOp::Lt,
            SsaExpr::var(v("loc0", 0)),
            SsaExpr::lit(Literal::Int(3)),
        ),
    ));
    // Keep t_0 live.
    block.add_stmt(assign_str(
        v("t", 1),
        SsaExpr::unresolved_call("use", vec![SsaExpr::var(v("t", 0))]),
    ));

    let mut blocks = BTreeMap::new();
    blocks.insert(BlockId(0), block);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
    optimize(&mut ssa);

    let b0 = ssa.block(BlockId(0)).unwrap();
    let cmp_stmt = b0
        .stmts
        .iter()
        .find(|s| matches!(s, SsaStmt::Assign { target, .. } if target == &v("t", 0)))
        .expect("t_0 def should survive");
    let SsaStmt::Assign { value, .. } = cmp_stmt else {
        panic!();
    };
    // The left operand must still be the slot VARIABLE, not Literal(0).
    let SsaExpr::Binary { left, .. } = value else {
        panic!("expected the comparison to survive, got {value:?}");
    };
    assert!(
        matches!(left.as_ref(), SsaExpr::Variable(var) if var == &v("loc0", 0)),
        "loc0_0 should stay symbolic in `loc0 < 3`, not be replaced by 0; got {value:?}"
    );
}

#[test]
fn propagates_vm_null_through_a_slot_load_alias() {
    let local = v("loc0", 0);
    let mut block = SsaBlock::new();
    block.add_stmt(assign_str(
        local.clone(),
        SsaExpr::var(SsaVariable::vm_null()),
    ));
    block.add_stmt(SsaStmt::ret(Some(SsaExpr::var(local.clone()))));

    let mut blocks = BTreeMap::new();
    blocks.insert(BlockId(0), block);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

    optimize(&mut ssa);

    let block = ssa.block(BlockId(0)).expect("entry block");
    assert!(!block
        .stmts
        .iter()
        .any(|statement| matches!(statement, SsaStmt::Assign { target, .. } if target == &local)));
    assert!(block.stmts.iter().any(|statement| matches!(
        statement,
        SsaStmt::Return(Some(SsaExpr::Variable(value))) if value.is_vm_null()
    )));
}

/// `(1 + 2)` then use of the result must fold to `3`.
#[test]
fn folds_constant_binary_and_propagates() {
    let mut block = SsaBlock::new();
    block.add_stmt(assign_str(v("b0", 0), SsaExpr::lit(Literal::Int(1))));
    block.add_stmt(assign_str(v("b0", 1), SsaExpr::lit(Literal::Int(2))));
    block.add_stmt(assign_str(
        v("b0", 2),
        SsaExpr::binary(
            BinOp::Add,
            SsaExpr::var(v("b0", 0)),
            SsaExpr::var(v("b0", 1)),
        ),
    ));
    block.add_stmt(assign_str(
        v("b0", 3),
        SsaExpr::unresolved_call("use", vec![SsaExpr::var(v("b0", 2))]),
    ));

    let mut blocks = BTreeMap::new();
    blocks.insert(BlockId(0), block);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

    let rounds = optimize(&mut ssa);
    assert!(rounds >= 1, "expected at least one optimization round");

    // The use site must now reference the folded literal 3 directly.
    let b0 = ssa.block(BlockId(0)).unwrap();
    let use_stmt = b0.stmts.last().unwrap();
    let SsaStmt::Assign { value, .. } = use_stmt else {
        panic!();
    };
    let SsaExpr::Call { args, .. } = value else {
        panic!("expected the use call, got {value:?}");
    };
    assert!(
        matches!(args[0], SsaExpr::Literal(Literal::Int(3))),
        "constant (1+2) should propagate as 3 into the use, got {:?}",
        args[0]
    );
}

#[test]
fn does_not_fold_negative_integer_exponents() {
    assert_eq!(
        fold_binary(BinOp::Pow, &Literal::Int(2), &Literal::Int(-1)),
        None
    );
}

#[test]
fn does_not_fold_i64_overflow_as_wrapping_vm_arithmetic() {
    assert_eq!(
        fold_binary(BinOp::Add, &Literal::Int(i64::MAX), &Literal::Int(1)),
        None
    );
    assert_eq!(fold_unary(UnaryOp::Neg, &Literal::Int(i64::MIN)), None);
    assert_eq!(
        fold_binary(BinOp::Shl, &Literal::Int(1), &Literal::Int(64)),
        None
    );
}

/// Copy chain `v1 = v0; v0 = 7` resolves so `v1`'s users see `7`.
#[test]
fn propagates_copy_chains() {
    let mut block = SsaBlock::new();
    block.add_stmt(assign_str(v("b0", 0), SsaExpr::lit(Literal::Int(7))));
    block.add_stmt(assign_str(v("b0", 1), SsaExpr::var(v("b0", 0))));
    block.add_stmt(assign_str(
        v("b0", 2),
        SsaExpr::unresolved_call("use", vec![SsaExpr::var(v("b0", 1))]),
    ));

    let mut blocks = BTreeMap::new();
    blocks.insert(BlockId(0), block);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
    optimize(&mut ssa);

    let b0 = ssa.block(BlockId(0)).unwrap();
    let SsaStmt::Assign { value, .. } = b0.stmts.last().unwrap() else {
        panic!();
    };
    let SsaExpr::Call { args, .. } = value else {
        panic!();
    };
    assert!(
        matches!(args[0], SsaExpr::Literal(Literal::Int(7))),
        "copy chain should resolve to 7, got {:?}",
        args[0]
    );
}

/// A trivial φ (single operand) collapses to that operand everywhere.
#[test]
fn eliminates_trivial_phi() {
    use crate::decompiler::cfg::ssa::PhiNode;
    let mut block = SsaBlock::new();
    // phi p0_0 = φ(BB1: b1_0)  — single operand, trivial.
    let mut phi = PhiNode::new(v("p0", 0));
    phi.add_operand(BlockId(1), v("b1", 0));
    block.add_phi(phi);
    // b1_0 = 5
    let mut pred = SsaBlock::new();
    pred.add_stmt(assign_str(v("b1", 0), SsaExpr::lit(Literal::Int(5))));
    // use of p0_0
    block.add_stmt(assign_str(
        v("b0", 0),
        SsaExpr::unresolved_call("use", vec![SsaExpr::var(v("p0", 0))]),
    ));

    let mut blocks = BTreeMap::new();
    blocks.insert(BlockId(0), block);
    blocks.insert(BlockId(1), pred);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
    optimize(&mut ssa);

    let b0 = ssa.block(BlockId(0)).unwrap();
    let SsaStmt::Assign { value, .. } = b0.stmts.last().unwrap() else {
        panic!();
    };
    let SsaExpr::Call { args, .. } = value else {
        panic!();
    };
    assert!(
        matches!(args[0], SsaExpr::Literal(Literal::Int(5))),
        "trivial phi should resolve through to 5, got {:?}",
        args[0]
    );
    assert!(
        ssa.block(BlockId(0))
            .expect("merge block")
            .phi_nodes
            .is_empty(),
        "substituted phi node must be removed"
    );
    assert!(!ssa.definitions.contains_key(&v("p0", 0)));
    assert_eq!(optimize(&mut ssa), 0, "optimized form must converge");
}

#[test]
fn retargets_terminator_use_when_removing_variable_trivial_phi() {
    let incoming = v("condition", 0);
    let target = v("merged_condition", 0);
    let merge_id = BlockId(0);
    let mut merge = SsaBlock::new();
    let mut phi = crate::decompiler::cfg::ssa::PhiNode::new(target.clone());
    phi.add_operand(BlockId(1), incoming.clone());
    merge.add_phi(phi);

    let mut predecessor = SsaBlock::new();
    predecessor.add_stmt(assign_str(
        incoming.clone(),
        SsaExpr::unresolved_call("condition", vec![]),
    ));

    let blocks = BTreeMap::from([(merge_id, merge), (BlockId(1), predecessor)]);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
    let terminator_use = UseSite::terminator(merge_id);
    ssa.uses
        .entry(target.clone())
        .or_default()
        .insert(terminator_use.clone());

    optimize(&mut ssa);

    assert!(ssa
        .block(merge_id)
        .expect("merge block")
        .phi_nodes
        .is_empty());
    assert!(!ssa.definitions.contains_key(&target));
    assert!(!ssa.uses.contains_key(&target));
    assert!(ssa.definitions.contains_key(&incoming));
    assert_eq!(
        ssa.uses.get(&incoming),
        Some(&BTreeSet::from([terminator_use]))
    );
    assert_eq!(optimize(&mut ssa), 0, "optimized form must converge");
}

#[test]
fn retargets_terminator_through_variable_trivial_phi_chain() {
    let incoming = v("condition", 0);
    let intermediate = v("intermediate_condition", 0);
    let target = v("merged_condition", 0);
    let merge_id = BlockId(0);
    let mut merge = SsaBlock::new();
    let mut outer_phi = crate::decompiler::cfg::ssa::PhiNode::new(target.clone());
    outer_phi.add_operand(BlockId(1), intermediate.clone());
    merge.add_phi(outer_phi);
    let mut inner_phi = crate::decompiler::cfg::ssa::PhiNode::new(intermediate.clone());
    inner_phi.add_operand(BlockId(1), incoming.clone());
    merge.add_phi(inner_phi);

    let mut predecessor = SsaBlock::new();
    predecessor.add_stmt(assign_str(
        incoming.clone(),
        SsaExpr::unresolved_call("condition", vec![]),
    ));

    let blocks = BTreeMap::from([(merge_id, merge), (BlockId(1), predecessor)]);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
    let terminator_use = UseSite::terminator(merge_id);
    ssa.uses
        .entry(target.clone())
        .or_default()
        .insert(terminator_use.clone());

    optimize(&mut ssa);

    assert!(ssa
        .block(merge_id)
        .expect("merge block")
        .phi_nodes
        .is_empty());
    assert!(!ssa.definitions.contains_key(&target));
    assert!(!ssa.definitions.contains_key(&intermediate));
    assert!(!ssa.uses.contains_key(&target));
    assert!(!ssa.uses.contains_key(&intermediate));
    assert!(ssa.definitions.contains_key(&incoming));
    assert_eq!(
        ssa.uses.get(&incoming),
        Some(&BTreeSet::from([terminator_use]))
    );
    assert_eq!(optimize(&mut ssa), 0, "optimized form must converge");
}

#[test]
fn rewrites_expression_through_variable_trivial_phi_chain() {
    let incoming = v("value", 0);
    let intermediate = v("intermediate_value", 0);
    let target = v("merged_value", 0);
    let mut merge = SsaBlock::new();
    let mut outer_phi = crate::decompiler::cfg::ssa::PhiNode::new(target.clone());
    outer_phi.add_operand(BlockId(1), intermediate.clone());
    merge.add_phi(outer_phi);
    let mut inner_phi = crate::decompiler::cfg::ssa::PhiNode::new(intermediate.clone());
    inner_phi.add_operand(BlockId(1), incoming.clone());
    merge.add_phi(inner_phi);
    merge.add_stmt(SsaStmt::expr(SsaExpr::unresolved_call(
        "use".to_string(),
        vec![SsaExpr::var(target.clone())],
    )));

    let mut predecessor = SsaBlock::new();
    predecessor.add_stmt(assign_str(
        incoming.clone(),
        SsaExpr::unresolved_call("value", vec![]),
    ));

    let blocks = BTreeMap::from([(BlockId(0), merge), (BlockId(1), predecessor)]);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

    optimize(&mut ssa);

    let effect = ssa
        .block(BlockId(0))
        .expect("merge block")
        .stmts
        .iter()
        .find_map(|stmt| match stmt {
            SsaStmt::Expr(SsaExpr::Call { target, args }) if target.display_name() == "use" => {
                Some(args)
            }
            _ => None,
        })
        .expect("effect use");
    assert_eq!(effect, &[SsaExpr::var(incoming.clone())]);
    assert!(!ssa.definitions.contains_key(&target));
    assert!(!ssa.definitions.contains_key(&intermediate));
    assert!(ssa.definitions.contains_key(&incoming));
    assert_eq!(optimize(&mut ssa), 0, "optimized form must converge");
}

#[test]
fn rewrites_expression_before_pruning_surviving_phi_operands() {
    let left = v("left", 0);
    let right = v("right", 0);
    let live_phi_target = v("live_phi", 0);
    let alias_target = v("alias", 0);
    let mut merge = SsaBlock::new();
    let mut live_phi = crate::decompiler::cfg::ssa::PhiNode::new(live_phi_target.clone());
    live_phi.add_operand(BlockId(1), left.clone());
    live_phi.add_operand(BlockId(2), right.clone());
    merge.add_phi(live_phi);
    let mut alias_phi = crate::decompiler::cfg::ssa::PhiNode::new(alias_target.clone());
    alias_phi.add_operand(BlockId(1), live_phi_target.clone());
    merge.add_phi(alias_phi);
    merge.add_stmt(SsaStmt::expr(SsaExpr::unresolved_call(
        "use".to_string(),
        vec![SsaExpr::var(alias_target.clone())],
    )));

    let mut left_predecessor = SsaBlock::new();
    left_predecessor.add_stmt(assign_str(left.clone(), SsaExpr::lit(Literal::Int(1))));
    let mut right_predecessor = SsaBlock::new();
    right_predecessor.add_stmt(assign_str(right.clone(), SsaExpr::lit(Literal::Int(2))));

    let blocks = BTreeMap::from([
        (BlockId(0), merge),
        (BlockId(1), left_predecessor),
        (BlockId(2), right_predecessor),
    ]);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

    optimize(&mut ssa);

    let merge = ssa.block(BlockId(0)).expect("merge block");
    assert!(merge
        .phi_nodes
        .iter()
        .any(|phi| phi.target == live_phi_target));
    assert!(!merge.phi_nodes.iter().any(|phi| phi.target == alias_target));
    let effect = merge
        .stmts
        .iter()
        .find_map(|stmt| match stmt {
            SsaStmt::Expr(SsaExpr::Call { target, args }) if target.display_name() == "use" => {
                Some(args)
            }
            _ => None,
        })
        .expect("effect use");
    assert_eq!(effect, &[SsaExpr::var(live_phi_target)]);
    assert!(ssa.definitions.contains_key(&left));
    assert!(ssa.definitions.contains_key(&right));
    assert_eq!(optimize(&mut ssa), 0, "optimized form must converge");
}

#[test]
fn leaves_rooted_cyclic_phis_stable() {
    let a = v("a", 0);
    let b = v("b", 0);
    let mut phi_a = crate::decompiler::cfg::ssa::PhiNode::new(a.clone());
    phi_a.add_operand(BlockId(1), b.clone());
    let mut phi_b = crate::decompiler::cfg::ssa::PhiNode::new(b.clone());
    phi_b.add_operand(BlockId(1), a.clone());
    let mut block = SsaBlock::new();
    block.add_phi(phi_a);
    block.add_phi(phi_b);
    block.add_stmt(SsaStmt::expr(SsaExpr::unresolved_call(
        "use".to_string(),
        vec![SsaExpr::var(a.clone())],
    )));

    let blocks = BTreeMap::from([(BlockId(0), block)]);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

    assert_eq!(
        one_round(&mut ssa),
        0,
        "cyclic substitutions are not rewrites"
    );
    assert_eq!(optimize(&mut ssa), 0, "cyclic phis must remain converged");
    assert!(ssa.definitions.contains_key(&a));
    assert!(ssa.definitions.contains_key(&b));
}

#[test]
fn preserves_literal_trivial_phi_used_by_terminator() {
    let incoming = v("condition", 0);
    let target = v("merged_condition", 0);
    let merge_id = BlockId(0);
    let mut merge = SsaBlock::new();
    let mut phi = crate::decompiler::cfg::ssa::PhiNode::new(target.clone());
    phi.add_operand(BlockId(1), incoming.clone());
    merge.add_phi(phi);

    let mut predecessor = SsaBlock::new();
    predecessor.add_stmt(assign_str(
        incoming.clone(),
        SsaExpr::lit(Literal::Bool(true)),
    ));

    let blocks = BTreeMap::from([(merge_id, merge), (BlockId(1), predecessor)]);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
    let terminator_use = UseSite::terminator(merge_id);
    ssa.uses
        .entry(target.clone())
        .or_default()
        .insert(terminator_use.clone());

    optimize(&mut ssa);

    assert!(ssa
        .block(merge_id)
        .expect("merge block")
        .phi_nodes
        .iter()
        .any(|phi| phi.target == target));
    assert!(ssa.definitions.contains_key(&target));
    assert_eq!(
        ssa.uses.get(&target),
        Some(&BTreeSet::from([terminator_use]))
    );
    assert_eq!(optimize(&mut ssa), 0, "optimized form must converge");
}

#[test]
fn preserves_literal_trivial_phi_used_by_nontrivial_phi() {
    let incoming = v("constant", 0);
    let literal_phi_target = v("literal_phi", 0);
    let other = v("other", 0);
    let live_phi_target = v("live_phi", 0);

    let mut merge = SsaBlock::new();
    let mut literal_phi = crate::decompiler::cfg::ssa::PhiNode::new(literal_phi_target.clone());
    literal_phi.add_operand(BlockId(1), incoming.clone());
    merge.add_phi(literal_phi);
    let mut live_phi = crate::decompiler::cfg::ssa::PhiNode::new(live_phi_target.clone());
    live_phi.add_operand(BlockId(1), literal_phi_target.clone());
    live_phi.add_operand(BlockId(2), other.clone());
    merge.add_phi(live_phi);
    merge.add_stmt(SsaStmt::expr(SsaExpr::unresolved_call(
        "use".to_string(),
        vec![SsaExpr::var(live_phi_target.clone())],
    )));

    let mut constant_predecessor = SsaBlock::new();
    constant_predecessor.add_stmt(assign_str(incoming, SsaExpr::lit(Literal::Int(7))));
    let mut other_predecessor = SsaBlock::new();
    other_predecessor.add_stmt(assign_str(other, SsaExpr::unresolved_call("other", vec![])));

    let blocks = BTreeMap::from([
        (BlockId(0), merge),
        (BlockId(1), constant_predecessor),
        (BlockId(2), other_predecessor),
    ]);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

    optimize(&mut ssa);

    let merge = ssa.block(BlockId(0)).expect("merge block");
    assert!(merge
        .phi_nodes
        .iter()
        .any(|phi| phi.target == literal_phi_target));
    let live_phi = merge
        .phi_nodes
        .iter()
        .find(|phi| phi.target == live_phi_target)
        .expect("live nontrivial phi");
    assert_eq!(
        live_phi.operands.get(&BlockId(1)),
        Some(&literal_phi_target)
    );
    assert!(ssa.definitions.contains_key(&literal_phi_target));
    assert_eq!(optimize(&mut ssa), 0, "optimized form must converge");
}

#[test]
fn removes_dead_phi_and_releases_operand_definition() {
    let incoming = v("b1", 0);
    let target = v("p0", 0);
    let mut merge = SsaBlock::new();
    let mut phi = crate::decompiler::cfg::ssa::PhiNode::new(target.clone());
    phi.add_operand(BlockId(1), incoming.clone());
    merge.add_phi(phi);
    merge.add_stmt(SsaStmt::ret(None));

    let mut predecessor = SsaBlock::new();
    predecessor.add_stmt(assign_str(incoming.clone(), SsaExpr::lit(Literal::Int(5))));

    let blocks = BTreeMap::from([(BlockId(0), merge), (BlockId(1), predecessor)]);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

    optimize(&mut ssa);

    assert!(ssa
        .block(BlockId(0))
        .expect("merge block")
        .phi_nodes
        .is_empty());
    assert!(!ssa.definitions.contains_key(&target));
    assert!(!ssa.definitions.contains_key(&incoming));
    assert!(!ssa.uses.contains_key(&incoming));
}

#[test]
fn converges_long_reverse_copy_chain_in_one_call() {
    let chain_len = 512usize;
    let mut block = SsaBlock::new();
    for index in 0..chain_len {
        let value = if index + 1 == chain_len {
            SsaExpr::lit(Literal::Int(7))
        } else {
            SsaExpr::var(v("t", index + 1))
        };
        block.add_stmt(assign_str(v("t", index), value));
    }
    block.add_stmt(SsaStmt::expr(SsaExpr::unresolved_call(
        "use".to_string(),
        vec![SsaExpr::var(v("t", 0))],
    )));
    block.add_stmt(SsaStmt::ret(None));

    let blocks = BTreeMap::from([(BlockId(0), block)]);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

    optimize(&mut ssa);

    let effect = ssa
        .block(BlockId(0))
        .expect("chain block")
        .stmts
        .iter()
        .find_map(|stmt| match stmt {
            SsaStmt::Expr(SsaExpr::Call { target, args }) if target.display_name() == "use" => {
                Some(args)
            }
            _ => None,
        })
        .expect("effect use");
    assert_eq!(effect, &[SsaExpr::lit(Literal::Int(7))]);
    assert_eq!(
        optimize(&mut ssa),
        0,
        "a single optimize call must reach its fixed point"
    );
}

#[test]
fn removes_dead_mutually_dependent_phi_component() {
    let a = v("a", 0);
    let b = v("b", 0);
    let mut phi_a = crate::decompiler::cfg::ssa::PhiNode::new(a.clone());
    phi_a.add_operand(BlockId(1), b.clone());
    phi_a.add_operand(BlockId(2), v("x", 0));
    let mut phi_b = crate::decompiler::cfg::ssa::PhiNode::new(b.clone());
    phi_b.add_operand(BlockId(1), a.clone());
    phi_b.add_operand(BlockId(2), v("y", 0));
    let mut block = SsaBlock::new();
    block.add_phi(phi_a);
    block.add_phi(phi_b);
    block.add_stmt(SsaStmt::ret(None));

    let blocks = BTreeMap::from([(BlockId(0), block)]);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

    optimize(&mut ssa);

    assert!(ssa
        .block(BlockId(0))
        .expect("phi block")
        .phi_nodes
        .is_empty());
    assert!(!ssa.definitions.contains_key(&a));
    assert!(!ssa.definitions.contains_key(&b));
    assert_eq!(optimize(&mut ssa), 0);
}

/// `v0 = 7` with no uses is dead and is removed.
#[test]
fn eliminates_dead_constant_def() {
    let mut block = SsaBlock::new();
    block.add_stmt(assign_str(v("b0", 0), SsaExpr::lit(Literal::Int(7))));
    // A *used* def so the block isn't empty and used-tracking is exercised.
    block.add_stmt(assign_str(
        v("b0", 1),
        SsaExpr::unresolved_call("use", vec![SsaExpr::lit(Literal::Int(1))]),
    ));

    let mut blocks = BTreeMap::new();
    blocks.insert(BlockId(0), block);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
    let before = ssa.block(BlockId(0)).unwrap().stmt_count();
    optimize(&mut ssa);
    let after = ssa.block(BlockId(0)).unwrap().stmt_count();
    assert!(after < before, "dead constant def b0#0 should be removed");
    assert!(!ssa.definitions.contains_key(&v("b0", 0)));
}

#[test]
fn effect_statement_keeps_input_definition_live() {
    let input = v("t", 0);
    let key = v("t", 1);
    let mut block = SsaBlock::new();
    block.add_stmt(assign_str(
        input.clone(),
        SsaExpr::unresolved_call("newmap", vec![]),
    ));
    block.add_stmt(assign_str(key.clone(), SsaExpr::lit(Literal::Int(1))));
    block.add_stmt(SsaStmt::expr(SsaExpr::unresolved_call(
        "set_item".to_string(),
        vec![
            SsaExpr::var(input.clone()),
            SsaExpr::var(key.clone()),
            SsaExpr::lit(Literal::Int(2)),
        ],
    )));
    block.add_stmt(SsaStmt::ret(None));

    let mut blocks = BTreeMap::new();
    blocks.insert(BlockId(0), block);
    let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
    let rounds = optimize(&mut ssa);

    assert!(
        rounds > 0,
        "the regression must exercise an optimization round"
    );

    let block = ssa.block(BlockId(0)).expect("test block exists");
    assert!(
        block.stmts.iter().any(|stmt| matches!(
            stmt,
            SsaStmt::Assign {
                target,
                value: SsaExpr::Call { target: call_target, args },
            } if target == &input
                && call_target.display_name() == "newmap"
                && args.is_empty()
        )),
        "effect input definition must survive optimization: {block:?}"
    );
    let effect_index = block
        .stmts
        .iter()
        .position(|stmt| {
            matches!(
                stmt,
                SsaStmt::Expr(SsaExpr::Call { target, .. })
                    if target.display_name() == "set_item"
            )
        })
        .expect("set_item effect statement must survive optimization");
    assert!(ssa.definitions.contains_key(&input));
    assert!(!ssa.definitions.contains_key(&key));
    assert!(ssa
        .uses
        .get(&input)
        .is_some_and(|sites| sites.iter().any(|site| site.stmt_index == effect_index)));
    assert!(block.stmts.iter().any(|stmt| matches!(
        stmt,
        SsaStmt::Expr(SsaExpr::Call { target, args })
            if target.display_name() == "set_item"
                && args.get(1) == Some(&SsaExpr::lit(Literal::Int(1)))
    )));
}

use super::{make_instr, BlockId, CfgBuilder, OpCode, Operand, Terminator};

#[test]
fn try_entry_adds_exception_and_finally_edges() {
    let instructions = vec![
        make_instr(0, OpCode::Try, Some(Operand::Bytes(vec![6, 9]))),
        make_instr(3, OpCode::Push1, None),
        make_instr(4, OpCode::Ret, None),
        make_instr(6, OpCode::Push2, None),
        make_instr(7, OpCode::Ret, None),
        make_instr(9, OpCode::Push3, None),
        make_instr(10, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();
    let entry = cfg.entry_block().unwrap();

    match &entry.terminator {
        Terminator::TryEntry {
            body_target,
            catch_target,
            finally_target,
        } => {
            assert_eq!(*body_target, BlockId::new(1));
            assert_eq!(*catch_target, Some(BlockId::new(2)));
            assert_eq!(*finally_target, Some(BlockId::new(3)));
        }
        other => panic!("expected TryEntry terminator, got {other:?}"),
    }

    let edges: Vec<_> = cfg
        .edges()
        .iter()
        .filter(|e| e.from == BlockId::ENTRY)
        .map(|e| (e.to, e.kind))
        .collect();
    assert!(edges.contains(&(
        BlockId::new(1),
        super::super::graph::EdgeKind::Unconditional
    )));
    assert!(edges.contains(&(BlockId::new(2), super::super::graph::EdgeKind::Exception)));
    assert!(edges.contains(&(
        BlockId::new(3),
        super::super::graph::EdgeKind::FinallyException
    )));
}

#[test]
fn endtry_routes_through_finally_to_the_natural_continuation() {
    let instructions = vec![
        make_instr(0, OpCode::Try, Some(Operand::Bytes(vec![0, 5]))),
        make_instr(3, OpCode::Push1, None),
        make_instr(4, OpCode::Endtry, Some(Operand::Jump(3))),
        make_instr(5, OpCode::Push2, None),
        make_instr(6, OpCode::Endfinally, None),
        make_instr(7, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();
    let endtry = cfg.block_at_offset(4).expect("ENDTRY block");
    let endfinally = cfg.block_at_offset(6).expect("ENDFINALLY block");
    let finally_target = cfg.block_at_offset(5).expect("finally block").id;
    let continuation = cfg.block_at_offset(7).expect("continuation block").id;

    assert!(matches!(
        endtry.terminator,
        Terminator::EndTryFinally {
            continuation: target,
            finally_target: finally,
            ..
        } if target == continuation && finally == finally_target
    ));
    assert!(matches!(
        endfinally.terminator,
        Terminator::EndFinally {
            ref normal_continuations,
        } if normal_continuations == &[continuation]
    ));
    assert_eq!(cfg.successors(endtry.id), &[finally_target]);
    assert_eq!(cfg.successors(endfinally.id), &[continuation]);
}

#[test]
fn nonlocal_finally_continuation_routes_to_its_explicit_target() {
    let instructions = vec![
        make_instr(0, OpCode::Try, Some(Operand::Bytes(vec![0, 5]))),
        make_instr(3, OpCode::Push1, None),
        make_instr(4, OpCode::Endtry, Some(Operand::Jump(4))),
        make_instr(5, OpCode::Push2, None),
        make_instr(6, OpCode::Endfinally, None),
        make_instr(7, OpCode::Nop, None),
        make_instr(8, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();
    let endtry = cfg.block_at_offset(4).expect("ENDTRY block");
    let endfinally = cfg.block_at_offset(6).expect("ENDFINALLY block");

    assert!(matches!(
        endtry.terminator,
        Terminator::EndTryFinally {
            continuation: target,
            nonlocal: true,
            ..
        } if target == BlockId::new(4)
    ));
    assert!(matches!(
        endfinally.terminator,
        Terminator::EndFinally {
            ref normal_continuations,
        } if normal_continuations == &[BlockId::new(4)]
    ));
    assert_eq!(cfg.successors(endfinally.id), &[BlockId::new(4)]);
}

#[test]
fn nested_endtry_at_inner_resume_boundary_routes_through_outer_finally() {
    let instructions = vec![
        make_instr(0, OpCode::Try, Some(Operand::Bytes(vec![0, 13]))),
        make_instr(3, OpCode::Try, Some(Operand::Bytes(vec![0, 6]))),
        make_instr(6, OpCode::Push1, None),
        make_instr(7, OpCode::Endtry, Some(Operand::Jump(4))),
        make_instr(9, OpCode::Push2, None),
        make_instr(10, OpCode::Endfinally, None),
        make_instr(11, OpCode::Endtry, Some(Operand::Jump(4))),
        make_instr(13, OpCode::Push3, None),
        make_instr(14, OpCode::Endfinally, None),
        make_instr(15, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();
    let inner_endtry = cfg.block_at_offset(7).expect("inner ENDTRY block");
    let inner_endfinally = cfg.block_at_offset(10).expect("inner ENDFINALLY block");
    let outer_endtry = cfg.block_at_offset(11).expect("outer ENDTRY block");
    let outer_endfinally = cfg.block_at_offset(14).expect("outer ENDFINALLY block");
    let inner_finally = cfg.block_at_offset(9).expect("inner finally block").id;
    let outer_finally = cfg.block_at_offset(13).expect("outer finally block").id;
    let continuation = cfg.block_at_offset(15).expect("continuation block").id;

    assert!(matches!(
        inner_endtry.terminator,
        Terminator::EndTryFinally {
            continuation: target,
            finally_target,
            ..
        } if target == outer_endtry.id && finally_target == inner_finally
    ));
    assert!(matches!(
        inner_endfinally.terminator,
        Terminator::EndFinally {
            ref normal_continuations,
        } if normal_continuations == &[outer_endtry.id]
    ));
    assert!(matches!(
        outer_endtry.terminator,
        Terminator::EndTryFinally {
            continuation: target,
            finally_target,
            ..
        } if target == continuation && finally_target == outer_finally
    ));
    assert!(matches!(
        outer_endfinally.terminator,
        Terminator::EndFinally {
            ref normal_continuations,
        } if normal_continuations == &[continuation]
    ));
    assert_eq!(cfg.successors(outer_endtry.id), &[outer_finally]);
    assert_eq!(cfg.successors(outer_endfinally.id), &[continuation]);
}

#[test]
fn nested_catch_resume_boundary_belongs_to_the_enclosing_try() {
    let instructions = vec![
        make_instr(0, OpCode::Try, Some(Operand::Bytes(vec![0, 20]))),
        make_instr(3, OpCode::Try, Some(Operand::Bytes(vec![6, 13]))),
        make_instr(6, OpCode::Push1, None),
        make_instr(7, OpCode::Endtry, Some(Operand::Jump(7))),
        make_instr(9, OpCode::Drop, None),
        make_instr(10, OpCode::Endtry, Some(Operand::Jump(4))),
        make_instr(12, OpCode::Nop, None),
        make_instr(13, OpCode::Nop, None),
        make_instr(14, OpCode::Endtry, Some(Operand::Jump(9))),
        make_instr(16, OpCode::Push2, None),
        make_instr(17, OpCode::Endfinally, None),
        make_instr(18, OpCode::Nop, None),
        make_instr(19, OpCode::Nop, None),
        make_instr(20, OpCode::Push3, None),
        make_instr(21, OpCode::Endfinally, None),
        make_instr(22, OpCode::Nop, None),
        make_instr(23, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();
    let inner_finally = cfg.block_at_offset(16).expect("inner finally block").id;
    let outer_endtry = cfg.block_at_offset(14).expect("outer ENDTRY block");
    let outer_finally = cfg.block_at_offset(20).expect("outer finally block").id;
    let continuation = cfg.block_at_offset(23).expect("continuation block").id;

    for offset in [7, 10] {
        let endtry = cfg.block_at_offset(offset).expect("inner ENDTRY block");
        assert!(matches!(
            endtry.terminator,
            Terminator::EndTryFinally {
                continuation: target,
                finally_target,
                ..
            } if target == outer_endtry.id && finally_target == inner_finally
        ));
    }
    assert!(matches!(
        outer_endtry.terminator,
        Terminator::EndTryFinally {
            continuation: target,
            finally_target,
            ..
        } if target == continuation && finally_target == outer_finally
    ));
    assert_eq!(cfg.successors(outer_endtry.id), &[outer_finally]);
}

#[test]
fn nested_body_resume_boundary_belongs_to_the_enclosing_catch() {
    let instructions = vec![
        make_instr(0, OpCode::Try, Some(Operand::Bytes(vec![6, 20]))),
        make_instr(3, OpCode::Push0, None),
        make_instr(4, OpCode::Endtry, Some(Operand::Jump(19))),
        make_instr(6, OpCode::Drop, None),
        make_instr(7, OpCode::Try, Some(Operand::Bytes(vec![0, 9]))),
        make_instr(10, OpCode::Endtry, Some(Operand::Jump(4))),
        make_instr(14, OpCode::Endtry, Some(Operand::Jump(9))),
        make_instr(16, OpCode::Push1, None),
        make_instr(17, OpCode::Endfinally, None),
        make_instr(20, OpCode::Push2, None),
        make_instr(21, OpCode::Endfinally, None),
        make_instr(23, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();
    let inner_endtry = cfg.block_at_offset(10).expect("inner ENDTRY block");
    let outer_endtry = cfg.block_at_offset(14).expect("outer ENDTRY block");
    let inner_finally = cfg.block_at_offset(16).expect("inner finally block").id;
    let outer_finally = cfg.block_at_offset(20).expect("outer finally block").id;
    let continuation = cfg.block_at_offset(23).expect("continuation block").id;

    assert!(matches!(
        inner_endtry.terminator,
        Terminator::EndTryFinally {
            continuation: target,
            finally_target,
            ..
        } if target == outer_endtry.id && finally_target == inner_finally
    ));
    assert!(matches!(
        outer_endtry.terminator,
        Terminator::EndTryFinally {
            continuation: target,
            finally_target,
            ..
        } if target == continuation && finally_target == outer_finally
    ));
    assert_eq!(cfg.successors(outer_endtry.id), &[outer_finally]);
}

#[test]
fn triple_nested_endtry_chain_unwinds_one_parent_at_a_time() {
    let instructions = vec![
        make_instr(0, OpCode::Try, Some(Operand::Bytes(vec![0, 25]))),
        make_instr(3, OpCode::Try, Some(Operand::Bytes(vec![0, 17]))),
        make_instr(6, OpCode::Try, Some(Operand::Bytes(vec![0, 10]))),
        make_instr(9, OpCode::Push1, None),
        make_instr(10, OpCode::Endtry, Some(Operand::Jump(2))),
        make_instr(12, OpCode::Endtry, Some(Operand::Jump(2))),
        make_instr(14, OpCode::Endtry, Some(Operand::Jump(15))),
        make_instr(16, OpCode::Push2, None),
        make_instr(17, OpCode::Endfinally, None),
        make_instr(18, OpCode::Nop, None),
        make_instr(19, OpCode::Nop, None),
        make_instr(20, OpCode::Push3, None),
        make_instr(21, OpCode::Endfinally, None),
        make_instr(22, OpCode::Nop, None),
        make_instr(23, OpCode::Nop, None),
        make_instr(24, OpCode::Nop, None),
        make_instr(25, OpCode::Push4, None),
        make_instr(26, OpCode::Endfinally, None),
        make_instr(27, OpCode::Nop, None),
        make_instr(28, OpCode::Nop, None),
        make_instr(29, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();
    let inner_endtry = cfg.block_at_offset(10).expect("inner ENDTRY block");
    let middle_endtry = cfg.block_at_offset(12).expect("middle ENDTRY block");
    let outer_endtry = cfg.block_at_offset(14).expect("outer ENDTRY block");
    let inner_finally = cfg.block_at_offset(16).expect("inner finally block").id;
    let middle_finally = cfg.block_at_offset(20).expect("middle finally block").id;
    let outer_finally = cfg.block_at_offset(25).expect("outer finally block").id;
    let continuation = cfg.block_at_offset(29).expect("continuation block").id;

    assert!(matches!(
        inner_endtry.terminator,
        Terminator::EndTryFinally {
            continuation: target,
            finally_target,
            ..
        } if target == middle_endtry.id && finally_target == inner_finally
    ));
    assert!(matches!(
        middle_endtry.terminator,
        Terminator::EndTryFinally {
            continuation: target,
            finally_target,
            ..
        } if target == outer_endtry.id && finally_target == middle_finally
    ));
    assert!(matches!(
        outer_endtry.terminator,
        Terminator::EndTryFinally {
            continuation: target,
            finally_target,
            ..
        } if target == continuation && finally_target == outer_finally
    ));
}

#[test]
fn shared_finally_dispatches_to_each_saved_normal_continuation() {
    let instructions = vec![
        make_instr(0, OpCode::Try, Some(Operand::Bytes(vec![0, 10]))),
        make_instr(3, OpCode::Push1, None),
        make_instr(4, OpCode::Jmpif, Some(Operand::Jump(4))),
        make_instr(6, OpCode::Endtry, Some(Operand::Jump(6))),
        make_instr(8, OpCode::Endtry, Some(Operand::Jump(6))),
        make_instr(10, OpCode::Push2, None),
        make_instr(11, OpCode::Endfinally, None),
        make_instr(12, OpCode::Ret, None),
        make_instr(13, OpCode::Nop, None),
        make_instr(14, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();
    let natural_endtry = cfg.block_at_offset(6).expect("natural ENDTRY block");
    let nonlocal_endtry = cfg.block_at_offset(8).expect("nonlocal ENDTRY block");
    let endfinally = cfg.block_at_offset(11).expect("ENDFINALLY block");
    let natural = cfg.block_at_offset(12).expect("natural continuation").id;
    let nonlocal = cfg.block_at_offset(14).expect("nonlocal continuation").id;

    assert!(matches!(
        natural_endtry.terminator,
        Terminator::EndTryFinally {
            continuation,
            nonlocal: false,
            ..
        } if continuation == natural
    ));
    assert!(matches!(
        nonlocal_endtry.terminator,
        Terminator::EndTryFinally {
            continuation,
            nonlocal: true,
            ..
        } if continuation == nonlocal
    ));
    assert!(matches!(
        endfinally.terminator,
        Terminator::EndFinally {
            ref normal_continuations,
        } if normal_continuations == &[natural, nonlocal]
    ));
    assert_eq!(cfg.successors(endfinally.id), &[natural, nonlocal]);
    assert!(
        cfg.edges()
            .iter()
            .filter(|edge| {
                edge.from == endfinally.id
                    && edge.kind == super::super::graph::EdgeKind::FinallyContinuation
            })
            .count()
            == 2
    );
}

#[test]
fn catch_region_distinguishes_natural_and_nonlocal_endtry_targets() {
    let instructions = vec![
        make_instr(0, OpCode::Try, Some(Operand::Bytes(vec![10, 0]))),
        make_instr(3, OpCode::Push1, None),
        make_instr(4, OpCode::Jmpif, Some(Operand::Jump(4))),
        make_instr(6, OpCode::Endtry, Some(Operand::Jump(10))),
        make_instr(8, OpCode::Endtry, Some(Operand::Jump(6))),
        make_instr(10, OpCode::Drop, None),
        make_instr(11, OpCode::Endtry, Some(Operand::Jump(3))),
        make_instr(13, OpCode::Nop, None),
        make_instr(14, OpCode::Nop, None),
        make_instr(15, OpCode::Ret, None),
        make_instr(16, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();
    let nonlocal = cfg.block_at_offset(6).expect("nonlocal ENDTRY block");
    let body_natural = cfg.block_at_offset(8).expect("body ENDTRY block");
    let catch_natural = cfg.block_at_offset(11).expect("catch ENDTRY block");

    assert!(matches!(
        nonlocal.terminator,
        Terminator::EndTry { nonlocal: true, .. }
    ));
    for block in [body_natural, catch_natural] {
        assert!(matches!(
            block.terminator,
            Terminator::EndTry {
                nonlocal: false,
                ..
            }
        ));
    }
}

#[test]
fn catch_endtry_defines_natural_continuation_when_try_body_throws() {
    let instructions = vec![
        make_instr(0, OpCode::Try, Some(Operand::Bytes(vec![6, 0]))),
        make_instr(3, OpCode::Push1, None),
        make_instr(4, OpCode::Throw, None),
        make_instr(6, OpCode::Drop, None),
        make_instr(7, OpCode::Endtry, Some(Operand::Jump(2))),
        make_instr(9, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();
    let catch_endtry = cfg.block_at_offset(7).expect("catch ENDTRY block");
    let continuation = cfg.block_at_offset(9).expect("natural continuation").id;

    assert!(matches!(
        catch_endtry.terminator,
        Terminator::EndTry {
            continuation: target,
            nonlocal: false,
        } if target == continuation
    ));
}

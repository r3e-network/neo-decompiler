use super::{make_instr, BlockId, CfgBuilder, OpCode, Operand, Terminator};

#[test]
fn try_entry_adds_exception_and_finally_edges() {
    let instructions = vec![
        make_instr(0, OpCode::Try, Some(Operand::Bytes(vec![3, 6]))),
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
    assert!(edges.contains(&(BlockId::new(3), super::super::graph::EdgeKind::Finally)));
}

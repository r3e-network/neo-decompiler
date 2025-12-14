use super::{make_instr, BlockId, CfgBuilder, OpCode, Operand, Terminator};

#[test]
fn unconditional_jump_creates_two_blocks() {
    let instructions = vec![
        make_instr(0, OpCode::Jmp, Some(Operand::Jump(1))), // jumps to offset 3
        make_instr(2, OpCode::Push0, None),                 // skipped
        make_instr(3, OpCode::Ret, None),                   // target
    ];

    let cfg = CfgBuilder::new(&instructions).build();

    assert!(cfg.block_count() >= 2);
}

#[test]
fn conditional_branch_creates_multiple_blocks() {
    let instructions = vec![
        make_instr(0, OpCode::Push1, None),
        make_instr(1, OpCode::Jmpifnot, Some(Operand::Jump(2))), // if false, jump to offset 5
        make_instr(3, OpCode::Push0, None),                      // then branch
        make_instr(4, OpCode::Ret, None),
        make_instr(5, OpCode::Push1, None), // else branch
        make_instr(6, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();

    assert!(cfg.block_count() >= 2);

    let entry = cfg.entry_block().unwrap();
    assert!(
        matches!(entry.terminator, Terminator::Branch { .. })
            || matches!(entry.terminator, Terminator::Fallthrough { .. })
    );
}

#[test]
fn multiple_exit_blocks() {
    let instructions = vec![
        make_instr(0, OpCode::Push1, None),
        make_instr(1, OpCode::Jmpifnot, Some(Operand::Jump(1))), // jump to offset 4
        make_instr(3, OpCode::Ret, None),                        // then: return
        make_instr(4, OpCode::Throw, None),                      // else: throw
    ];

    let cfg = CfgBuilder::new(&instructions).build();

    assert!(cfg.exit_blocks().len() >= 2);
}

#[test]
fn successors_and_predecessors() {
    let instructions = vec![
        make_instr(0, OpCode::Push1, None),
        make_instr(1, OpCode::Jmpifnot, Some(Operand::Jump(1))), // jump to offset 4
        make_instr(3, OpCode::Ret, None),
        make_instr(4, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();

    let entry_succs = cfg.successors(BlockId::ENTRY);
    assert!(!entry_succs.is_empty());

    for succ in entry_succs {
        let preds = cfg.predecessors(succ);
        assert!(preds.contains(&BlockId::ENTRY));
    }
}

#[test]
fn edge_count_matches_terminators() {
    let instructions = vec![
        make_instr(0, OpCode::Push1, None),
        make_instr(1, OpCode::Jmpifnot, Some(Operand::Jump(1))), // conditional branch
        make_instr(3, OpCode::Ret, None),
        make_instr(4, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();

    let entry_edges: Vec<_> = cfg
        .edges()
        .iter()
        .filter(|e| e.from == BlockId::ENTRY)
        .collect();
    assert_eq!(entry_edges.len(), 2);
}

#[test]
fn long_jump_creates_blocks() {
    let instructions = vec![
        make_instr(0, OpCode::Jmp_L, Some(Operand::Jump32(5))), // jumps to offset 10
        make_instr(5, OpCode::Push0, None),                     // skipped
        make_instr(6, OpCode::Ret, None),
        make_instr(10, OpCode::Push1, None), // target
        make_instr(11, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();

    assert!(cfg.block_count() >= 2);
}

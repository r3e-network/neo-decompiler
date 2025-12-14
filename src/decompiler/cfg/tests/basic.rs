use super::{make_instr, BlockId, CfgBuilder, OpCode, Terminator};

#[test]
fn empty_instructions_produces_empty_cfg() {
    let cfg = CfgBuilder::new(&[]).build();
    assert_eq!(cfg.block_count(), 0);
}

#[test]
fn single_block_linear_code() {
    let instructions = vec![
        make_instr(0, OpCode::Push0, None),
        make_instr(1, OpCode::Push1, None),
        make_instr(2, OpCode::Add, None),
        make_instr(3, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();

    assert_eq!(cfg.block_count(), 1);
    let entry = cfg.entry_block().unwrap();
    assert_eq!(entry.id, BlockId::ENTRY);
    assert!(matches!(entry.terminator, Terminator::Return));
}

#[test]
fn block_contains_offset() {
    let instructions = vec![
        make_instr(0, OpCode::Push0, None),
        make_instr(1, OpCode::Push1, None),
        make_instr(2, OpCode::Add, None),
        make_instr(3, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();

    for offset in 0..4 {
        let block = cfg.block_at_offset(offset);
        assert!(block.is_some(), "offset {} should be in a block", offset);
        assert_eq!(block.unwrap().id, BlockId::ENTRY);
    }
}

#[test]
fn block_instruction_count() {
    let instructions = vec![
        make_instr(0, OpCode::Push0, None),
        make_instr(1, OpCode::Push1, None),
        make_instr(2, OpCode::Push2, None),
        make_instr(3, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();
    let entry = cfg.entry_block().unwrap();

    assert_eq!(entry.instruction_count(), 4);
    assert!(!entry.is_empty());
}

#[test]
fn block_id_display() {
    let id = BlockId::new(42);
    assert_eq!(format!("{}", id), "BB42");
    assert_eq!(id.index(), 42);
}

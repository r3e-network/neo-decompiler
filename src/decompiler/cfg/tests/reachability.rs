use super::{make_instr, BlockId, CfgBuilder, OpCode, Operand};

#[test]
fn unreachable_blocks_detect_dead_code_after_jump() {
    let instructions = vec![
        make_instr(0, OpCode::Jmp, Some(Operand::Jump(1))),
        make_instr(2, OpCode::Push0, None), // unreachable
        make_instr(3, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();

    let unreachable = cfg.unreachable_blocks();
    assert!(unreachable.contains(&BlockId::new(1)));
    assert!(!unreachable.contains(&BlockId::ENTRY));
    assert!(cfg.is_reachable(BlockId::ENTRY));
}

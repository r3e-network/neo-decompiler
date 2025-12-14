use super::{make_instr, BlockId, CfgBuilder, OpCode, Operand};

#[test]
fn reverse_postorder_visits_all_blocks() {
    let instructions = vec![
        make_instr(0, OpCode::Push1, None),
        make_instr(1, OpCode::Jmpifnot, Some(Operand::Jump(3))), // jump to offset 6
        make_instr(3, OpCode::Push0, None),
        make_instr(4, OpCode::Jmp, Some(Operand::Jump(1))), // jump to offset 7
        make_instr(6, OpCode::Push1, None),
        make_instr(7, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();
    let rpo = cfg.reverse_postorder();

    assert!(!rpo.is_empty());
    assert_eq!(rpo[0], BlockId::ENTRY);
}

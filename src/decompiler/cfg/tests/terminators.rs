use super::{BlockId, Terminator};

#[test]
fn throw_creates_exit_block() {
    let instructions = vec![
        super::make_instr(0, super::OpCode::Push0, None),
        super::make_instr(1, super::OpCode::Throw, None),
    ];

    let cfg = super::CfgBuilder::new(&instructions).build();

    assert_eq!(cfg.block_count(), 1);
    let entry = cfg.entry_block().unwrap();
    assert!(matches!(entry.terminator, Terminator::Throw));
    assert!(cfg.exit_blocks().contains(&entry.id));
}

#[test]
fn abort_creates_exit_block() {
    let instructions = vec![
        super::make_instr(0, super::OpCode::Push0, None),
        super::make_instr(1, super::OpCode::Abort, None),
    ];

    let cfg = super::CfgBuilder::new(&instructions).build();

    assert_eq!(cfg.block_count(), 1);
    let entry = cfg.entry_block().unwrap();
    assert!(matches!(entry.terminator, Terminator::Abort));
    assert!(cfg.exit_blocks().contains(&entry.id));
}

#[test]
fn terminator_successors() {
    let ret = Terminator::Return;
    assert!(ret.successors().is_empty());

    let jump = Terminator::Jump {
        target: BlockId::new(1),
    };
    assert_eq!(jump.successors(), vec![BlockId::new(1)]);

    let branch = Terminator::Branch {
        then_target: BlockId::new(1),
        else_target: BlockId::new(2),
    };
    assert_eq!(branch.successors(), vec![BlockId::new(1), BlockId::new(2)]);
}

#[test]
fn terminator_properties() {
    let fallthrough = Terminator::Fallthrough {
        target: BlockId::new(1),
    };
    assert!(fallthrough.can_fallthrough());
    assert!(!fallthrough.is_conditional());

    let branch = Terminator::Branch {
        then_target: BlockId::new(1),
        else_target: BlockId::new(2),
    };
    assert!(!branch.can_fallthrough());
    assert!(branch.is_conditional());
}

#[test]
fn endtry_is_modeled_as_endtry_terminator() {
    let instructions = vec![
        super::make_instr(0, super::OpCode::Push0, None),
        super::make_instr(1, super::OpCode::Endtry, Some(super::Operand::Jump(2))),
        super::make_instr(3, super::OpCode::Push1, None), // dead code
        super::make_instr(4, super::OpCode::Ret, None),
        super::make_instr(5, super::OpCode::Ret, None), // continuation
    ];

    let cfg = super::CfgBuilder::new(&instructions).build();
    let entry = cfg.entry_block().unwrap();
    assert!(matches!(entry.terminator, Terminator::EndTry { .. }));
    assert!(!cfg.unreachable_blocks().is_empty());
}

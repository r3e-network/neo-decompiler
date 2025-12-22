use super::*;
use crate::instruction::Operand;

#[test]
fn decodes_simple_sequence() {
    let bytecode = [0x10, 0x11, 0x9E, 0x40];
    let instructions = Disassembler::new()
        .disassemble(&bytecode)
        .expect("disassembly succeeds");

    let mnemonics: Vec<_> = instructions
        .iter()
        .map(|ins| ins.opcode.mnemonic())
        .collect();
    assert_eq!(mnemonics, vec!["PUSH0", "PUSH1", "ADD", "RET"]);
}

#[test]
fn errors_on_unknown_opcode() {
    let bytecode = [0xFF];
    let err = Disassembler::with_unknown_handling(UnknownHandling::Error)
        .disassemble(&bytecode)
        .unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Disassembly(DisassemblyError::UnknownOpcode {
            opcode: 0xFF,
            offset: 0
        })
    ));
}

#[test]
fn permits_unknown_opcode_when_configured() {
    let bytecode = [0xFF, 0x40];
    let instructions = Disassembler::with_unknown_handling(UnknownHandling::Permit)
        .disassemble(&bytecode)
        .expect("disassembly succeeds in tolerant mode");

    assert_eq!(instructions.len(), 2);
    assert!(matches!(instructions[0].opcode, OpCode::Unknown(0xFF)));
    assert_eq!(instructions[1].opcode, OpCode::Ret);
}

#[test]
fn reports_warning_for_unknown_opcode_in_tolerant_mode() {
    let bytecode = [0xFF, 0x40];
    let output = Disassembler::with_unknown_handling(UnknownHandling::Permit)
        .disassemble_with_warnings(&bytecode)
        .expect("disassembly succeeds in tolerant mode");

    assert_eq!(output.instructions.len(), 2);
    assert_eq!(output.warnings.len(), 1);
    assert!(matches!(
        output.warnings[0],
        DisassemblyWarning::UnknownOpcode {
            opcode: 0xFF,
            offset: 0
        }
    ));
}

#[test]
fn fails_on_truncated_operand() {
    let bytecode = [0x01, 0x00];
    let err = Disassembler::new().disassemble(&bytecode).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Disassembly(DisassemblyError::UnexpectedEof { offset: 0 })
    ));
}

#[test]
fn decodes_calla_operand() {
    let bytecode = [0x36, 0x34, 0x12];
    let instructions = Disassembler::new()
        .disassemble(&bytecode)
        .expect("disassembly succeeds");

    assert_eq!(instructions.len(), 1);
    assert_eq!(instructions[0].operand, Some(Operand::U16(0x1234)));
}

#[test]
fn decodes_pushdata2() {
    let bytecode = [0x0D, 0x04, 0x00, 0xDE, 0xAD, 0xBE, 0xEF];
    let instruction = Disassembler::new().disassemble(&bytecode).expect("success")[0].clone();
    assert_eq!(instruction.opcode.mnemonic(), "PUSHDATA2");
    assert_eq!(instruction.offset, 0);
    assert_eq!(
        instruction.operand,
        Some(Operand::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]))
    );
    assert_eq!(instruction.opcode, OpCode::Pushdata2);
}

#[test]
fn decodes_jump_long() {
    let bytecode = [0x23, 0x34, 0x12, 0x00, 0x00];
    let instruction = Disassembler::new().disassemble(&bytecode).expect("success")[0].clone();
    assert_eq!(instruction.opcode, OpCode::Jmp_L);
    assert_eq!(instruction.operand, Some(Operand::Jump32(0x1234)));
    assert_eq!(instruction.offset, 0);
}

#[test]
fn decodes_syscall_operand_with_name() {
    // System.Runtime.Platform
    let bytecode = [0x41, 0xB2, 0x79, 0xFC, 0xF6];
    let instruction = Disassembler::new().disassemble(&bytecode).expect("success")[0].clone();
    assert_eq!(instruction.opcode, OpCode::Syscall);
    let operand = instruction.operand.expect("syscall operand");
    assert_eq!(operand.to_string(), "System.Runtime.Platform (0xF6FC79B2)");
}

#[test]
fn pushdata4_excessive_length_returns_operand_too_large() {
    // PUSHDATA4 length is little-endian u32.
    // MAX_OPERAND_LEN is 1_048_576, so request one more than that.
    let bytecode = [0x0E, 0x01, 0x00, 0x10, 0x00];
    let err = Disassembler::new().disassemble(&bytecode).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Disassembly(DisassemblyError::OperandTooLarge {
            offset: 0,
            len: 1_048_577
        })
    ));
}

#[test]
fn pushdata4_truncated_payload_returns_unexpected_eof() {
    // PUSHDATA4 claims a 4-byte payload but only provides 2 bytes.
    let bytecode = [0x0E, 0x04, 0x00, 0x00, 0x00, 0xAA, 0xBB];
    let err = Disassembler::new().disassemble(&bytecode).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Disassembly(DisassemblyError::UnexpectedEof { offset: 0 })
    ));
}

use crate::error::{DisassemblyError, Result};
use crate::instruction::{Instruction, OpCode, Operand};

/// Stateless helper that decodes Neo VM bytecode into structured instructions.
#[derive(Debug, Default, Clone, Copy)]
pub struct Disassembler;

impl Disassembler {
    pub fn new() -> Self {
        Self
    }

    /// Disassemble an entire bytecode buffer.
    pub fn disassemble(&self, bytecode: &[u8]) -> Result<Vec<Instruction>> {
        let mut instructions = Vec::new();
        let mut pc = 0usize;

        while pc < bytecode.len() {
            let (instruction, size) = self.decode_instruction(bytecode, pc)?;
            instructions.push(instruction);
            pc += size;
        }

        Ok(instructions)
    }

    fn decode_instruction(&self, bytecode: &[u8], offset: usize) -> Result<(Instruction, usize)> {
        let opcode_byte = *bytecode
            .get(offset)
            .ok_or(DisassemblyError::UnexpectedEof { offset })?;
        let opcode = OpCode::from_byte(opcode_byte);

        match opcode {
            OpCode::PushInt8 => {
                let value = *bytecode
                    .get(offset + 1)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::I8(value as i8))),
                    2,
                ))
            }
            OpCode::PushInt16 => {
                let bytes = bytecode
                    .get(offset + 1..offset + 3)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                let value = i16::from_le_bytes(bytes.try_into().unwrap());
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::I16(value))),
                    3,
                ))
            }
            OpCode::PushInt32 => {
                let bytes = bytecode
                    .get(offset + 1..offset + 5)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                let value = i32::from_le_bytes(bytes.try_into().unwrap());
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::I32(value))),
                    5,
                ))
            }
            OpCode::PushInt64 => {
                let bytes = bytecode
                    .get(offset + 1..offset + 9)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                let value = i64::from_le_bytes(bytes.try_into().unwrap());
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::I64(value))),
                    9,
                ))
            }
            OpCode::PushInt128 => {
                let bytes = bytecode
                    .get(offset + 1..offset + 17)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::Bytes(bytes.to_vec()))),
                    17,
                ))
            }
            OpCode::PushInt256 => {
                let bytes = bytecode
                    .get(offset + 1..offset + 33)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::Bytes(bytes.to_vec()))),
                    33,
                ))
            }
            OpCode::PushData1 => {
                let len = *bytecode
                    .get(offset + 1)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?
                    as usize;
                let start = offset + 2;
                let end = start + len;
                let bytes = bytecode
                    .get(start..end)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::Bytes(bytes.to_vec()))),
                    2 + len,
                ))
            }
            OpCode::PushData2 => {
                let len_bytes = bytecode
                    .get(offset + 1..offset + 3)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                let len = u16::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
                let start = offset + 3;
                let end = start + len;
                let bytes = bytecode
                    .get(start..end)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::Bytes(bytes.to_vec()))),
                    3 + len,
                ))
            }
            OpCode::PushData4 => {
                let len_bytes = bytecode
                    .get(offset + 1..offset + 5)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
                let start = offset + 5;
                let end = start + len;
                let bytes = bytecode
                    .get(start..end)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::Bytes(bytes.to_vec()))),
                    5 + len,
                ))
            }
            OpCode::Jump | OpCode::JumpIf => {
                let value = *bytecode
                    .get(offset + 1)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::Jump(value as i8))),
                    2,
                ))
            }
            OpCode::JumpLong | OpCode::JumpIfLong => {
                let bytes = bytecode
                    .get(offset + 1..offset + 5)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                let value = i32::from_le_bytes(bytes.try_into().unwrap());
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::Jump32(value))),
                    5,
                ))
            }
            OpCode::Call => {
                let value = *bytecode
                    .get(offset + 1)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::Jump(value as i8))),
                    2,
                ))
            }
            OpCode::CallLong => {
                let bytes = bytecode
                    .get(offset + 1..offset + 5)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                let value = i32::from_le_bytes(bytes.try_into().unwrap());
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::Jump32(value))),
                    5,
                ))
            }
            OpCode::CallA | OpCode::CallT => {
                let bytes = bytecode
                    .get(offset + 1..offset + 3)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                let value = u16::from_le_bytes(bytes.try_into().unwrap());
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::U16(value))),
                    3,
                ))
            }
            OpCode::Syscall => {
                let bytes = bytecode
                    .get(offset + 1..offset + 5)
                    .ok_or(DisassemblyError::UnexpectedEof { offset })?;
                let hash = u32::from_le_bytes(bytes.try_into().unwrap());
                Ok((
                    Instruction::new(offset, opcode, Some(Operand::Syscall(hash))),
                    5,
                ))
            }
            OpCode::Unknown(byte) => Err(DisassemblyError::UnknownOpcode {
                opcode: byte,
                offset,
            }
            .into()),
            // All remaining supported opcodes are single byte without operands.
            _ => Ok((Instruction::new(offset, opcode, None), 1)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn rejects_unknown_opcode() {
        let bytecode = [0xFF];
        let err = Disassembler::new().disassemble(&bytecode).unwrap_err();
        assert!(matches!(
            err,
            crate::error::Error::Disassembly(DisassemblyError::UnknownOpcode { opcode: 0xFF, .. })
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
    fn decodes_pushdata2() {
        let bytecode = [0x0D, 0x04, 0x00, 0xDE, 0xAD, 0xBE, 0xEF];
        let instruction = Disassembler::new().disassemble(&bytecode).expect("success")[0].clone();
        assert_eq!(instruction.opcode.mnemonic(), "PUSHDATA2");
        assert_eq!(instruction.offset, 0);
        assert_eq!(
            instruction.operand,
            Some(Operand::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]))
        );
        assert_eq!(instruction.opcode, OpCode::PushData2);
    }

    #[test]
    fn decodes_jump_long() {
        let bytecode = [0x23, 0x34, 0x12, 0x00, 0x00];
        let instruction = Disassembler::new().disassemble(&bytecode).expect("success")[0].clone();
        assert_eq!(instruction.opcode, OpCode::JumpLong);
        assert_eq!(instruction.operand, Some(Operand::Jump32(0x1234)));
        assert_eq!(instruction.offset, 0);
    }
}

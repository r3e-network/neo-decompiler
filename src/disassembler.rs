use crate::error::{DisassemblyError, Result};
use crate::instruction::{Instruction, OpCode, Operand, OperandEncoding};

/// How to handle unknown opcode bytes encountered during disassembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownHandling {
    /// Surface an error as soon as an unknown opcode is encountered.
    Error,
    /// Emit an `Unknown` instruction and continue disassembling subsequent bytes.
    Permit,
}

/// Stateless helper that decodes Neo VM bytecode into structured instructions.
#[derive(Debug, Clone, Copy)]
pub struct Disassembler {
    unknown: UnknownHandling,
}

impl Default for Disassembler {
    fn default() -> Self {
        Self::new()
    }
}

impl Disassembler {
    pub fn new() -> Self {
        Self {
            unknown: UnknownHandling::Permit,
        }
    }

    pub fn with_unknown_handling(unknown: UnknownHandling) -> Self {
        Self { unknown }
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
        if let OpCode::Unknown(_) = opcode {
            return match self.unknown {
                UnknownHandling::Permit => Ok((Instruction::new(offset, opcode, None), 1)),
                UnknownHandling::Error => Err(DisassemblyError::UnknownOpcode {
                    opcode: opcode_byte,
                    offset,
                }
                .into()),
            };
        }

        let (operand, consumed) = self.read_operand(opcode, bytecode, offset)?;
        Ok((Instruction::new(offset, opcode, operand), 1 + consumed))
    }

    fn read_operand(
        &self,
        opcode: OpCode,
        bytecode: &[u8],
        offset: usize,
    ) -> Result<(Option<Operand>, usize)> {
        let immediate_constant = match opcode {
            OpCode::PushM1 => Some(Operand::I32(-1)),
            OpCode::Push0 => Some(Operand::I32(0)),
            OpCode::Push1 => Some(Operand::I32(1)),
            OpCode::Push2 => Some(Operand::I32(2)),
            OpCode::Push3 => Some(Operand::I32(3)),
            OpCode::Push4 => Some(Operand::I32(4)),
            OpCode::Push5 => Some(Operand::I32(5)),
            OpCode::Push6 => Some(Operand::I32(6)),
            OpCode::Push7 => Some(Operand::I32(7)),
            OpCode::Push8 => Some(Operand::I32(8)),
            OpCode::Push9 => Some(Operand::I32(9)),
            OpCode::Push10 => Some(Operand::I32(10)),
            OpCode::Push11 => Some(Operand::I32(11)),
            OpCode::Push12 => Some(Operand::I32(12)),
            OpCode::Push13 => Some(Operand::I32(13)),
            OpCode::Push14 => Some(Operand::I32(14)),
            OpCode::Push15 => Some(Operand::I32(15)),
            OpCode::Push16 => Some(Operand::I32(16)),
            OpCode::PushT => Some(Operand::Bool(true)),
            OpCode::PushF => Some(Operand::Bool(false)),
            OpCode::PushNull => Some(Operand::Null),
            _ => None,
        };
        if let Some(constant) = immediate_constant {
            return Ok((Some(constant), 0));
        }

        match opcode.operand_encoding() {
            OperandEncoding::None => Ok((None, 0)),
            OperandEncoding::I8 => {
                let bytes = self.read_slice(bytecode, offset + 1, 1, offset)?;
                let value = bytes[0] as i8;
                Ok((Some(Operand::I8(value)), 1))
            }
            OperandEncoding::I16 => {
                let bytes = self.read_slice(bytecode, offset + 1, 2, offset)?;
                let value = i16::from_le_bytes(bytes.try_into().unwrap());
                Ok((Some(Operand::I16(value)), 2))
            }
            OperandEncoding::I32 => {
                let bytes = self.read_slice(bytecode, offset + 1, 4, offset)?;
                let value = i32::from_le_bytes(bytes.try_into().unwrap());
                Ok((Some(Operand::I32(value)), 4))
            }
            OperandEncoding::I64 => {
                let bytes = self.read_slice(bytecode, offset + 1, 8, offset)?;
                let value = i64::from_le_bytes(bytes.try_into().unwrap());
                Ok((Some(Operand::I64(value)), 8))
            }
            OperandEncoding::Bytes(len) => {
                let bytes = self.read_slice(bytecode, offset + 1, len, offset)?.to_vec();
                Ok((Some(Operand::Bytes(bytes)), len))
            }
            OperandEncoding::Data1 => {
                let len_bytes = self.read_slice(bytecode, offset + 1, 1, offset)?;
                let len = len_bytes[0] as usize;
                let data = self.read_slice(bytecode, offset + 2, len, offset)?.to_vec();
                Ok((Some(Operand::Bytes(data)), 1 + len))
            }
            OperandEncoding::Data2 => {
                let len_bytes = self.read_slice(bytecode, offset + 1, 2, offset)?;
                let len = u16::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
                let data = self.read_slice(bytecode, offset + 3, len, offset)?.to_vec();
                Ok((Some(Operand::Bytes(data)), 2 + len))
            }
            OperandEncoding::Data4 => {
                let len_bytes = self.read_slice(bytecode, offset + 1, 4, offset)?;
                let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
                let data = self.read_slice(bytecode, offset + 5, len, offset)?.to_vec();
                Ok((Some(Operand::Bytes(data)), 4 + len))
            }
            OperandEncoding::Jump8 => {
                let bytes = self.read_slice(bytecode, offset + 1, 1, offset)?;
                let value = bytes[0] as i8;
                Ok((Some(Operand::Jump(value)), 1))
            }
            OperandEncoding::Jump32 => {
                let bytes = self.read_slice(bytecode, offset + 1, 4, offset)?;
                let value = i32::from_le_bytes(bytes.try_into().unwrap());
                Ok((Some(Operand::Jump32(value)), 4))
            }
            OperandEncoding::U16 => {
                let bytes = self.read_slice(bytecode, offset + 1, 2, offset)?;
                let value = u16::from_le_bytes(bytes.try_into().unwrap());
                Ok((Some(Operand::U16(value)), 2))
            }
            OperandEncoding::U8 => {
                let bytes = self.read_slice(bytecode, offset + 1, 1, offset)?;
                Ok((Some(Operand::U8(bytes[0])), 1))
            }
            OperandEncoding::U32 => {
                let bytes = self.read_slice(bytecode, offset + 1, 4, offset)?;
                let value = u32::from_le_bytes(bytes.try_into().unwrap());
                Ok((Some(Operand::U32(value)), 4))
            }
            OperandEncoding::Syscall => {
                let bytes = self.read_slice(bytecode, offset + 1, 4, offset)?;
                let value = u32::from_le_bytes(bytes.try_into().unwrap());
                Ok((Some(Operand::Syscall(value)), 4))
            }
        }
    }

    fn read_slice<'a>(
        &self,
        bytecode: &'a [u8],
        start: usize,
        len: usize,
        offset: usize,
    ) -> Result<&'a [u8]> {
        match bytecode.get(start..start + len) {
            Some(slice) => Ok(slice),
            None => Err(DisassemblyError::UnexpectedEof { offset }.into()),
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
}

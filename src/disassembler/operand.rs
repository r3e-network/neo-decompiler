use crate::error::{DisassemblyError, Result};
use crate::instruction::{OpCode, Operand, OperandEncoding};

use super::Disassembler;

const MAX_OPERAND_LEN: usize = 1_048_576;

impl Disassembler {
    pub(super) fn read_operand(
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
                let (bytes, consumed) = self.read_bytes_prefixed(bytecode, offset, 1)?;
                Ok((Some(Operand::Bytes(bytes)), consumed))
            }
            OperandEncoding::Data2 => {
                let (bytes, consumed) = self.read_bytes_prefixed(bytecode, offset, 2)?;
                Ok((Some(Operand::Bytes(bytes)), consumed))
            }
            OperandEncoding::Data4 => {
                let (bytes, consumed) = self.read_bytes_prefixed(bytecode, offset, 4)?;
                Ok((Some(Operand::Bytes(bytes)), consumed))
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

    fn read_bytes_prefixed(
        &self,
        bytecode: &[u8],
        offset: usize,
        prefix_len: usize,
    ) -> Result<(Vec<u8>, usize)> {
        let len_bytes = self.read_slice(bytecode, offset + 1, prefix_len, offset)?;
        let len = match prefix_len {
            1 => len_bytes[0] as usize,
            2 => u16::from_le_bytes(len_bytes.try_into().unwrap()) as usize,
            4 => u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize,
            _ => unreachable!("prefix_len is controlled by OperandEncoding"),
        };

        if len > MAX_OPERAND_LEN {
            return Err(DisassemblyError::OperandTooLarge { offset, len }.into());
        }

        let data_start = offset + 1 + prefix_len;
        let data = self.read_slice(bytecode, data_start, len, offset)?.to_vec();
        Ok((data, prefix_len + len))
    }

    fn read_slice<'a>(
        &self,
        bytecode: &'a [u8],
        start: usize,
        len: usize,
        offset: usize,
    ) -> Result<&'a [u8]> {
        let end = start
            .checked_add(len)
            .ok_or(DisassemblyError::UnexpectedEof { offset })?;
        match bytecode.get(start..end) {
            Some(slice) => Ok(slice),
            None => Err(DisassemblyError::UnexpectedEof { offset }.into()),
        }
    }
}

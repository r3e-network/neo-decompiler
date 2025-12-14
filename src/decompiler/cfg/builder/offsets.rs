use crate::instruction::{Instruction, Operand, OperandEncoding};

use super::super::basic_block::BlockId;
use super::CfgBuilder;

impl<'a> CfgBuilder<'a> {
    pub(super) fn offset_to_block_id(&self, offset: usize, leaders: &[usize]) -> BlockId {
        for (i, &leader) in leaders.iter().enumerate().rev() {
            if leader <= offset {
                return BlockId::new(i);
            }
        }
        BlockId::ENTRY
    }

    pub(super) fn instruction_end_offset(&self, index: usize) -> Option<usize> {
        self.instructions.get(index + 1).map(|ins| ins.offset)
    }

    pub(super) fn instruction_len_fallback(&self, instr: &Instruction) -> usize {
        match instr.opcode.operand_encoding() {
            OperandEncoding::None => 1,
            OperandEncoding::I8 | OperandEncoding::U8 | OperandEncoding::Jump8 => 2,
            OperandEncoding::I16 | OperandEncoding::U16 => 3,
            OperandEncoding::I32
            | OperandEncoding::U32
            | OperandEncoding::Jump32
            | OperandEncoding::Syscall => 5,
            OperandEncoding::I64 => 9,
            OperandEncoding::Bytes(n) => 1 + n,
            OperandEncoding::Data1 => 1 + 1 + Self::bytes_len(instr),
            OperandEncoding::Data2 => 1 + 2 + Self::bytes_len(instr),
            OperandEncoding::Data4 => 1 + 4 + Self::bytes_len(instr),
        }
    }

    fn bytes_len(instr: &Instruction) -> usize {
        match &instr.operand {
            Some(Operand::Bytes(bytes)) => bytes.len(),
            _ => 0,
        }
    }

    pub(super) fn end_offset(&self) -> usize {
        let Some(last) = self.instructions.last() else {
            return 0;
        };
        let last_index = self.instructions.len().saturating_sub(1);
        self.instruction_end_offset(last_index)
            .unwrap_or_else(|| last.offset + self.instruction_len_fallback(last))
    }
}

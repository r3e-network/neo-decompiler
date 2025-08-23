//! Neo N3 bytecode disassembler

use crate::common::{config::DecompilerConfig, errors::DisassemblyError, types::*};

/// Neo N3 bytecode disassembler
pub struct Disassembler {
    /// Instruction decoder
    decoder: InstructionDecoder,
    /// Configuration
    config: DecompilerConfig,
}

impl Disassembler {
    /// Create new disassembler with configuration
    pub fn new(config: &DecompilerConfig) -> Self {
        Self {
            decoder: InstructionDecoder::new(),
            config: config.clone(),
        }
    }

    /// Disassemble bytecode into instruction stream
    pub fn disassemble(&self, bytecode: &[u8]) -> Result<Vec<Instruction>, DisassemblyError> {
        let mut instructions = Vec::new();
        let mut offset = 0u32;

        while (offset as usize) < bytecode.len() {
            match self
                .decoder
                .decode_instruction(&bytecode[(offset as usize)..], offset)
            {
                Ok(instruction) => {
                    offset += instruction.size as u32;
                    instructions.push(instruction);
                }
                Err(DisassemblyError::TruncatedInstruction { .. }) => {
                    // Handle truncated instruction gracefully
                    if (offset as usize) < bytecode.len() {
                        let opcode = OpCode::from_byte(bytecode[offset as usize]);
                        let instruction = Instruction {
                            offset,
                            opcode,
                            operand: None,
                            size: 1, // Minimum size
                        };
                        instructions.push(instruction);
                        offset += 1;
                    } else {
                        break;
                    }
                }
                Err(other_error) => return Err(other_error),
            }
        }

        Ok(instructions)
    }
}

/// Instruction decoder for Neo N3 bytecode
pub struct InstructionDecoder;

impl InstructionDecoder {
    /// Create new instruction decoder
    pub fn new() -> Self {
        Self
    }

    /// Decode single instruction at offset
    pub fn decode_instruction(
        &self,
        data: &[u8],
        offset: u32,
    ) -> Result<Instruction, DisassemblyError> {
        if data.is_empty() {
            return Err(DisassemblyError::TruncatedInstruction { offset });
        }

        let opcode_byte = data[0];
        let opcode = OpCode::from_byte(opcode_byte);

        let (operand, operand_size) = self.decode_operand(&opcode, &data[1..], offset)?;

        // Total instruction size is opcode byte + operand bytes
        // Use saturating arithmetic to prevent overflow and cap at u8::MAX
        let total_size = (1usize + operand_size as usize).min(u8::MAX as usize) as u8;

        // Validate that we have enough data for the complete instruction
        if (1 + operand_size as usize) > data.len() {
            // For truncated instructions, try to create a valid instruction with available data
            let available_size = data.len().saturating_sub(1);
            let safe_size = available_size.min(operand_size as usize) as u8;
            
            return Ok(Instruction {
                offset,
                opcode,
                operand: None, // Clear operand for truncated instruction
                size: 1 + safe_size,
            });
        }

        Ok(Instruction {
            offset,
            opcode,
            operand,
            size: total_size,
        })
    }

    /// Decode instruction operand based on opcode
    fn decode_operand(
        &self,
        opcode: &OpCode,
        data: &[u8],
        offset: u32,
    ) -> Result<(Option<Operand>, u8), DisassemblyError> {
        match opcode {
            // === INTEGER PUSH OPERATIONS ===
            OpCode::PUSHINT8 => {
                if data.is_empty() {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                Ok((Some(Operand::Integer(data[0] as i8 as i64)), 1))
            }

            OpCode::PUSHINT16 => {
                if data.len() < 2 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let value = i16::from_le_bytes([data[0], data[1]]) as i64;
                Ok((Some(Operand::Integer(value)), 2))
            }

            OpCode::PUSHINT32 => {
                if data.len() < 4 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let value = i32::from_le_bytes([data[0], data[1], data[2], data[3]]) as i64;
                Ok((Some(Operand::Integer(value)), 4))
            }

            OpCode::PUSHINT64 => {
                if data.len() < 8 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let bytes = [
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ];
                let value = i64::from_le_bytes(bytes);
                Ok((Some(Operand::Integer(value)), 8))
            }

            OpCode::PUSHINT128 => {
                if data.len() < 16 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let bytes = data[0..16].to_vec();
                Ok((Some(Operand::BigInteger(bytes)), 16))
            }

            OpCode::PUSHINT256 => {
                if data.len() < 32 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let bytes = data[0..32].to_vec();
                Ok((Some(Operand::BigInteger(bytes)), 32))
            }

            // === DATA PUSH OPERATIONS ===
            OpCode::PUSHDATA1 => {
                if data.is_empty() {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let len = data[0] as usize;
                if data.len() < 1 + len {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let bytes = data[1..1 + len].to_vec();
                Ok((Some(Operand::Bytes(bytes)), 1 + len as u8))
            }

            OpCode::PUSHDATA2 => {
                if data.len() < 2 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let len = u16::from_le_bytes([data[0], data[1]]) as usize;
                if data.len() < 2 + len {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let bytes = data[2..2 + len].to_vec();
                // Cap the operand size to prevent u8 overflow
                let operand_size = (2 + len).min(u8::MAX as usize - 1) as u8;
                Ok((Some(Operand::Bytes(bytes)), operand_size))
            }

            OpCode::PUSHDATA4 => {
                if data.len() < 4 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
                if data.len() < 4 + len {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let bytes = data[4..4 + len].to_vec();
                // Cap the operand size to prevent u8 overflow
                let operand_size = (4 + len).min(u8::MAX as usize - 1) as u8;
                Ok((Some(Operand::Bytes(bytes)), operand_size))
            }

            // === JUMP OPERATIONS (Short form - 1 byte offset) ===
            OpCode::JMP
            | OpCode::JMPIF
            | OpCode::JMPIFNOT
            | OpCode::JMPEQ
            | OpCode::JMPNE
            | OpCode::JMPGT
            | OpCode::JMPGE
            | OpCode::JMPLT
            | OpCode::JMPLE => {
                if data.is_empty() {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let target = data[0] as i8;
                Ok((Some(Operand::JumpTarget8(target)), 1))
            }

            // === JUMP OPERATIONS (Long form - 4 byte offset) ===
            OpCode::JMP_L
            | OpCode::JMPIF_L
            | OpCode::JMPIFNOT_L
            | OpCode::JMPEQ_L
            | OpCode::JMPNE_L
            | OpCode::JMPGT_L
            | OpCode::JMPGE_L
            | OpCode::JMPLT_L
            | OpCode::JMPLE_L => {
                if data.len() < 4 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let target = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                Ok((Some(Operand::JumpTarget32(target)), 4))
            }

            // === CALL OPERATIONS ===
            OpCode::CALL => {
                if data.is_empty() {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let target = data[0] as i8;
                Ok((Some(Operand::JumpTarget8(target)), 1))
            }

            OpCode::CALL_L => {
                if data.len() < 4 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let target = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                Ok((Some(Operand::JumpTarget32(target)), 4))
            }

            OpCode::CALLA => {
                if data.len() < 2 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let token = u16::from_le_bytes([data[0], data[1]]);
                Ok((Some(Operand::MethodToken(token)), 2))
            }

            OpCode::CALLT => {
                if data.len() < 2 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let token = u16::from_le_bytes([data[0], data[1]]);
                Ok((Some(Operand::CallToken(token)), 2))
            }

            // === SYSCALL ===
            OpCode::SYSCALL => {
                if data.len() < 4 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let hash = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                Ok((Some(Operand::SyscallHash(hash)), 4))
            }

            // === TRY-CATCH-FINALLY ===
            OpCode::TRY => {
                if data.len() < 2 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let catch_offset = data[0] as i8 as u32;
                let finally_offset = if data[1] == 0 {
                    None
                } else {
                    Some(data[1] as i8 as u32)
                };
                Ok((
                    Some(Operand::TryBlock {
                        catch_offset,
                        finally_offset,
                    }),
                    2,
                ))
            }

            OpCode::TRY_L => {
                if data.len() < 8 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let catch_offset = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                let finally_bytes = [data[4], data[5], data[6], data[7]];
                let finally_offset = if finally_bytes == [0, 0, 0, 0] {
                    None
                } else {
                    Some(u32::from_le_bytes(finally_bytes))
                };
                Ok((
                    Some(Operand::TryBlock {
                        catch_offset,
                        finally_offset,
                    }),
                    8,
                ))
            }

            OpCode::ENDTRY => {
                if data.is_empty() {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let target = data[0] as i8;
                Ok((Some(Operand::JumpTarget8(target)), 1))
            }

            OpCode::ENDTRY_L => {
                if data.len() < 4 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let target = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                Ok((Some(Operand::JumpTarget32(target)), 4))
            }

            // === STACK OPERATIONS ===
            OpCode::XDROP => {
                if data.is_empty() {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                Ok((Some(Operand::Count(data[0])), 1))
            }

            OpCode::PICK | OpCode::ROLL => {
                if data.is_empty() {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                Ok((Some(Operand::SlotIndex(data[0])), 1))
            }

            OpCode::REVERSEN => {
                if data.is_empty() {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                Ok((Some(Operand::Count(data[0])), 1))
            }

            // === SLOT OPERATIONS ===
            OpCode::INITSSLOT => {
                if data.is_empty() {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                Ok((Some(Operand::Count(data[0])), 1))
            }

            OpCode::INITSLOT => {
                if data.len() < 2 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                Ok((
                    Some(Operand::SlotInit {
                        local_slots: data[0],
                        static_slots: data[1],
                    }),
                    2,
                ))
            }

            OpCode::LDSFLD
            | OpCode::STSFLD
            | OpCode::LDLOC
            | OpCode::STLOC
            | OpCode::LDARG
            | OpCode::STARG => {
                if data.is_empty() {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                Ok((Some(Operand::SlotIndex(data[0])), 1))
            }

            // === STRING/ARRAY OPERATIONS ===
            OpCode::NEWBUFFER => {
                if data.len() < 2 {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let size = u16::from_le_bytes([data[0], data[1]]);
                Ok((Some(Operand::BufferSize(size)), 2))
            }

            OpCode::NEWARRAY | OpCode::NEWARRAYT | OpCode::NEWSTRUCT => {
                if data.is_empty() {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                Ok((Some(Operand::Count(data[0])), 1))
            }

            // === TYPE OPERATIONS ===
            OpCode::CONVERT | OpCode::ISTYPE => {
                if data.is_empty() {
                    return Err(DisassemblyError::TruncatedInstruction { offset });
                }
                let stack_type = self.decode_stack_item_type(data[0], offset)?;
                Ok((Some(Operand::StackItemType(stack_type)), 1))
            }

            // === MESSAGE OPERATIONS ===
            OpCode::ABORTMSG | OpCode::ASSERTMSG => {
                // ABORTMSG and ASSERTMSG don't take operands
                // The message should already be on the stack from a previous PUSHDATA instruction
                Ok((None, 0))
            }

            // === INSTRUCTIONS WITHOUT OPERANDS ===
            OpCode::PUSHT
            | OpCode::PUSHF
            | OpCode::PUSHM1
            | OpCode::PUSHNULL
            | OpCode::PUSH0
            | OpCode::PUSH1
            | OpCode::PUSH2
            | OpCode::PUSH3
            | OpCode::PUSH4
            | OpCode::PUSH5
            | OpCode::PUSH6
            | OpCode::PUSH7
            | OpCode::PUSH8
            | OpCode::PUSH9
            | OpCode::PUSH10
            | OpCode::PUSH11
            | OpCode::PUSH12
            | OpCode::PUSH13
            | OpCode::PUSH14
            | OpCode::PUSH15
            | OpCode::PUSH16
            | OpCode::NOP
            | OpCode::RET
            | OpCode::ABORT
            | OpCode::ASSERT
            | OpCode::THROW
            | OpCode::ENDFINALLY
            | OpCode::DEPTH
            | OpCode::DROP
            | OpCode::NIP
            | OpCode::CLEAR
            | OpCode::DUP
            | OpCode::OVER
            | OpCode::TUCK
            | OpCode::SWAP
            | OpCode::ROT
            | OpCode::REVERSE3
            | OpCode::REVERSE4
            | OpCode::LDSFLD0
            | OpCode::LDSFLD1
            | OpCode::LDSFLD2
            | OpCode::LDSFLD3
            | OpCode::LDSFLD4
            | OpCode::LDSFLD5
            | OpCode::LDSFLD6
            | OpCode::LDLOC0
            | OpCode::LDLOC1
            | OpCode::LDLOC2
            | OpCode::LDLOC3
            | OpCode::LDLOC4
            | OpCode::LDLOC5
            | OpCode::LDLOC6
            | OpCode::LDARG0
            | OpCode::LDARG1
            | OpCode::LDARG2
            | OpCode::LDARG3
            | OpCode::LDARG4
            | OpCode::LDARG5
            | OpCode::LDARG6
            | OpCode::MEMCPY
            | OpCode::CAT
            | OpCode::SUBSTR
            | OpCode::LEFT
            | OpCode::RIGHT
            | OpCode::SIZE
            | OpCode::INVERT
            | OpCode::AND
            | OpCode::OR
            | OpCode::XOR
            | OpCode::EQUAL
            | OpCode::NOTEQUAL
            | OpCode::SIGN
            | OpCode::ABS
            | OpCode::NEGATE
            | OpCode::INC
            | OpCode::DEC
            | OpCode::ADD
            | OpCode::SUB
            | OpCode::MUL
            | OpCode::DIV
            | OpCode::MOD
            | OpCode::POW
            | OpCode::SQRT
            | OpCode::MODMUL
            | OpCode::MODPOW
            | OpCode::SHL
            | OpCode::SHR
            | OpCode::NOT
            | OpCode::BOOLAND
            | OpCode::BOOLOR
            | OpCode::NZ
            | OpCode::NUMEQUAL
            | OpCode::NUMNOTEQUAL
            | OpCode::LT
            | OpCode::LE
            | OpCode::GT
            | OpCode::GE
            | OpCode::MIN
            | OpCode::MAX
            | OpCode::WITHIN
            | OpCode::PACKMAP
            | OpCode::PACKSTRUCT
            | OpCode::PACKARRAY
            | OpCode::UNPACK
            | OpCode::NEWARRAY0
            | OpCode::NEWSTRUCT0
            | OpCode::NEWMAP
            | OpCode::APPEND
            | OpCode::SETITEM
            | OpCode::PICKITEM
            | OpCode::REMOVE
            | OpCode::CLEARITEMS
            | OpCode::POPITEM
            | OpCode::HASKEY
            | OpCode::KEYS
            | OpCode::VALUES
            | OpCode::SLICE
            | OpCode::ISNULL => Ok((None, 0)),

            // === UNKNOWN OPCODES ===
            OpCode::UNKNOWN(byte) => Err(DisassemblyError::UnknownOpcode {
                opcode: *byte,
                offset,
            }),

            // === UNHANDLED OPCODES ===
            _ => {
                // For any remaining opcodes, assume no operand
                Ok((None, 0))
            }
        }
    }

    /// Decode stack item type from byte value
    fn decode_stack_item_type(
        &self,
        byte: u8,
        offset: u32,
    ) -> Result<StackItemType, DisassemblyError> {
        match byte {
            0x00 => Ok(StackItemType::Any),
            0x10 => Ok(StackItemType::Boolean),
            0x21 => Ok(StackItemType::Integer),
            0x28 => Ok(StackItemType::ByteString),
            0x30 => Ok(StackItemType::Buffer),
            0x40 => Ok(StackItemType::Array),
            0x41 => Ok(StackItemType::Struct),
            0x48 => Ok(StackItemType::Map),
            0x60 => Ok(StackItemType::InteropInterface),
            0x70 => Ok(StackItemType::Pointer),
            _ => Err(DisassemblyError::InvalidOperand {
                opcode: "CONVERT/ISTYPE".to_string(),
                offset,
            }),
        }
    }
}

impl Default for InstructionDecoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disassembler_creation() {
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);
        // Should create successfully
        let _ = disassembler;
    }

    #[test]
    fn test_decode_pushint8() {
        let decoder = InstructionDecoder::new();
        let bytecode = &[0x00, 0x42]; // PUSHINT8 66
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();

        assert_eq!(instruction.opcode, OpCode::PUSHINT8);
        assert_eq!(instruction.operand, Some(Operand::Integer(66)));
        assert_eq!(instruction.size, 2);
    }

    #[test]
    fn test_decode_jump() {
        let decoder = InstructionDecoder::new();
        let bytecode = &[0x22, 0x10]; // JMP 16 (short form)
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();

        assert_eq!(instruction.opcode, OpCode::JMP);
        assert_eq!(instruction.operand, Some(Operand::JumpTarget8(16)));
        assert_eq!(instruction.size, 2);
    }

    #[test]
    fn test_decode_jump_long() {
        let decoder = InstructionDecoder::new();
        let bytecode = &[0x23, 0x10, 0x00, 0x00, 0x00]; // JMP_L 16
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();

        assert_eq!(instruction.opcode, OpCode::JMP_L);
        assert_eq!(instruction.operand, Some(Operand::JumpTarget32(16)));
        assert_eq!(instruction.size, 5);
    }

    #[test]
    fn test_decode_syscall() {
        let decoder = InstructionDecoder::new();
        let bytecode = &[0x41, 0x12, 0x34, 0x56, 0x78]; // SYSCALL 0x78563412
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();

        assert_eq!(instruction.opcode, OpCode::SYSCALL);
        assert_eq!(instruction.operand, Some(Operand::SyscallHash(0x78563412)));
        assert_eq!(instruction.size, 5);
    }

    #[test]
    fn test_decode_simple_operation() {
        let decoder = InstructionDecoder::new();
        let bytecode = &[0x08]; // PUSHT (no operand)
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();

        assert_eq!(instruction.opcode, OpCode::PUSHT);
        assert_eq!(instruction.operand, None);
        assert_eq!(instruction.size, 1);
    }

    #[test]
    fn test_disassemble_sequence() {
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);

        // Simple sequence: PUSHINT8 42, PUSHINT8 10, ADD, RET
        let bytecode = &[
            0x00, 0x2A, // PUSHINT8 42
            0x00, 0x0A, // PUSHINT8 10
            0x8E, // ADD
            0x40, // RET
        ];

        let result = disassembler.disassemble(bytecode);
        assert!(result.is_ok());

        let instructions = result.unwrap();
        assert_eq!(instructions.len(), 4);

        assert_eq!(instructions[0].opcode, OpCode::PUSHINT8);
        assert_eq!(instructions[1].opcode, OpCode::PUSHINT8);
        assert_eq!(instructions[2].opcode, OpCode::ADD);
        assert_eq!(instructions[3].opcode, OpCode::RET);
    }

    #[test]
    fn test_truncated_instruction() {
        let decoder = InstructionDecoder::new();
        let bytecode = &[0x01]; // PUSHINT16 but no data
        let result = decoder.decode_instruction(bytecode, 0);

        assert!(matches!(
            result,
            Err(DisassemblyError::TruncatedInstruction { .. })
        ));
    }

    #[test]
    fn test_decode_pushdata2() {
        let decoder = InstructionDecoder::new();
        let bytecode = &[0x0D, 0x04, 0x00, 0x01, 0x02, 0x03, 0x04]; // PUSHDATA2 with 4 bytes
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();

        assert_eq!(instruction.opcode, OpCode::PUSHDATA2);
        assert_eq!(
            instruction.operand,
            Some(Operand::Bytes(vec![0x01, 0x02, 0x03, 0x04]))
        );
        assert_eq!(instruction.size, 7);
    }

    #[test]
    fn test_decode_initslot() {
        let decoder = InstructionDecoder::new();
        let bytecode = &[0x57, 0x03, 0x02]; // INITSLOT 3 locals, 2 statics
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();

        assert_eq!(instruction.opcode, OpCode::INITSLOT);
        assert_eq!(
            instruction.operand,
            Some(Operand::SlotInit {
                local_slots: 3,
                static_slots: 2
            })
        );
        assert_eq!(instruction.size, 3);
    }

    #[test]
    fn test_decode_try_catch() {
        let decoder = InstructionDecoder::new();
        let bytecode = &[0x3B, 0x0A, 0x14]; // TRY catch_offset=10, finally_offset=20
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();

        assert_eq!(instruction.opcode, OpCode::TRY);
        if let Some(Operand::TryBlock {
            catch_offset,
            finally_offset,
        }) = instruction.operand
        {
            assert_eq!(catch_offset, 10);
            assert_eq!(finally_offset, Some(20));
        } else {
            return Err(DisassemblyError::InvalidOperandType {
                expected: "TryBlock".to_string(),
                offset,
            });
        }
        assert_eq!(instruction.size, 3);
    }

    #[test]
    fn test_decode_convert() {
        let decoder = InstructionDecoder::new();
        let bytecode = &[0xC2, 0x21]; // CONVERT to Integer
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();

        assert_eq!(instruction.opcode, OpCode::CONVERT);
        assert_eq!(
            instruction.operand,
            Some(Operand::StackItemType(StackItemType::Integer))
        );
        assert_eq!(instruction.size, 2);
    }

    #[test]
    fn test_decode_push_constants() {
        let decoder = InstructionDecoder::new();

        // Test PUSH0
        let bytecode = &[0x10];
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();
        assert_eq!(instruction.opcode, OpCode::PUSH0);
        assert_eq!(instruction.operand, None);
        assert_eq!(instruction.size, 1);

        // Test PUSH16
        let bytecode = &[0x20];
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();
        assert_eq!(instruction.opcode, OpCode::PUSH16);
        assert_eq!(instruction.operand, None);
        assert_eq!(instruction.size, 1);
    }
}

//! Additional comprehensive tests for the Neo N3 instruction decoder

#[cfg(test)]
mod comprehensive_tests {
    use super::super::disassembler::*;
    use crate::common::{types::*, config::DecompilerConfig};

    #[test]
    fn test_decode_big_integers() {
        let decoder = InstructionDecoder::new();
        
        // Test PUSHINT64
        let bytecode = &[0x03, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F]; // PUSHINT64 max
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();
        assert_eq!(instruction.opcode, OpCode::PUSHINT64);
        assert_eq!(instruction.operand, Some(Operand::Integer(i64::MAX)));
        assert_eq!(instruction.size, 9);
        
        // Test PUSHINT128
        let mut bytecode128 = vec![0x04]; // PUSHINT128
        bytecode128.extend(vec![0x01; 16]); // 16 bytes of 0x01
        let instruction = decoder.decode_instruction(&bytecode128, 0).unwrap();
        assert_eq!(instruction.opcode, OpCode::PUSHINT128);
        if let Some(Operand::BigInteger(bytes)) = instruction.operand {
            assert_eq!(bytes.len(), 16);
            assert_eq!(bytes, vec![0x01; 16]);
        } else {
            assert!(false, "Expected BigInteger operand");
        }
        assert_eq!(instruction.size, 17);
    }

    #[test]
    fn test_decode_complex_jumps() {
        let decoder = InstructionDecoder::new();
        
        // Test conditional jump short form
        let bytecode = &[0x24, 0x0A]; // JMPIF with offset 10
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();
        assert_eq!(instruction.opcode, OpCode::JMPIF);
        assert_eq!(instruction.operand, Some(Operand::JumpTarget8(10)));
        assert_eq!(instruction.size, 2);
        
        // Test conditional jump long form
        let bytecode = &[0x25, 0x00, 0x01, 0x00, 0x00]; // JMPIF_L with offset 256
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();
        assert_eq!(instruction.opcode, OpCode::JMPIF_L);
        assert_eq!(instruction.operand, Some(Operand::JumpTarget32(256)));
        assert_eq!(instruction.size, 5);
    }

    #[test]
    fn test_decode_calls() {
        let decoder = InstructionDecoder::new();
        
        // Test CALLA with method token
        let bytecode = &[0x36, 0x12, 0x34]; // CALLA with token 0x3412
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();
        assert_eq!(instruction.opcode, OpCode::CALLA);
        assert_eq!(instruction.operand, Some(Operand::MethodToken(0x3412)));
        assert_eq!(instruction.size, 3);
        
        // Test CALLT with call token
        let bytecode = &[0x37, 0x56, 0x78]; // CALLT with token 0x7856
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();
        assert_eq!(instruction.opcode, OpCode::CALLT);
        assert_eq!(instruction.operand, Some(Operand::CallToken(0x7856)));
        assert_eq!(instruction.size, 3);
    }

    #[test] 
    fn test_decode_array_operations() {
        let decoder = InstructionDecoder::new();
        
        // Test NEWARRAY with count
        let bytecode = &[0xAD, 0x05]; // NEWARRAY with 5 elements
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();
        assert_eq!(instruction.opcode, OpCode::NEWARRAY);
        assert_eq!(instruction.operand, Some(Operand::Count(5)));
        assert_eq!(instruction.size, 2);
        
        // Test NEWBUFFER with size
        let bytecode = &[0x73, 0x00, 0x04]; // NEWBUFFER with size 1024
        let instruction = decoder.decode_instruction(bytecode, 0).unwrap();
        assert_eq!(instruction.opcode, OpCode::NEWBUFFER);
        assert_eq!(instruction.operand, Some(Operand::BufferSize(1024)));
        assert_eq!(instruction.size, 3);
    }

    #[test]
    fn test_decode_comprehensive_sequence() {
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);
        
        // Complex sequence with various instruction types
        let bytecode = &[
            0x57, 0x02, 0x01,        // INITSLOT 2 locals, 1 static
            0x00, 0x2A,              // PUSHINT8 42
            0x69, 0x00,              // STLOC 0
            0x68, 0x00,              // LDLOC 0
            0x11,                    // PUSH1
            0x8E,                    // ADD
            0x69, 0x01,              // STLOC 1
            0x24, 0x05,              // JMPIF 5 (conditional jump)
            0x40,                    // RET
            0x68, 0x01,              // LDLOC 1
            0x40,                    // RET
        ];
        
        let result = disassembler.disassemble(bytecode);
        assert!(result.is_ok());
        
        let instructions = result.unwrap();
        assert_eq!(instructions.len(), 8);
        
        // Verify each instruction
        assert_eq!(instructions[0].opcode, OpCode::INITSLOT);
        if let Some(Operand::SlotInit { local_slots, static_slots }) = &instructions[0].operand {
            assert_eq!(*local_slots, 2);
            assert_eq!(*static_slots, 1);
        } else {
            assert!(false, "Expected SlotInit operand");
        }
        
        assert_eq!(instructions[1].opcode, OpCode::PUSHINT8);
        assert_eq!(instructions[2].opcode, OpCode::STLOC);
        assert_eq!(instructions[3].opcode, OpCode::LDLOC);
        assert_eq!(instructions[4].opcode, OpCode::PUSH1);
        assert_eq!(instructions[5].opcode, OpCode::ADD);
        assert_eq!(instructions[6].opcode, OpCode::STLOC);
        assert_eq!(instructions[7].opcode, OpCode::JMPIF);
    }

    #[test]
    fn test_error_handling() {
        let decoder = InstructionDecoder::new();
        
        // Test unknown opcode
        let bytecode = &[0xFF]; // Unknown opcode
        let result = decoder.decode_instruction(bytecode, 0);
        assert!(matches!(result, Err(DisassemblyError::UnknownOpcode { .. })));
        
        // Test truncated PUSHINT32
        let bytecode = &[0x02, 0x12, 0x34]; // PUSHINT32 with only 2 bytes
        let result = decoder.decode_instruction(bytecode, 0);
        assert!(matches!(result, Err(DisassemblyError::TruncatedInstruction { .. })));
        
        // Test truncated PUSHDATA1
        let bytecode = &[0x0C, 0x05, 0x01, 0x02]; // PUSHDATA1 claims 5 bytes but only has 2
        let result = decoder.decode_instruction(bytecode, 0);
        assert!(matches!(result, Err(DisassemblyError::TruncatedInstruction { .. })));
    }

    #[test]
    fn test_opcode_properties() {
        // Test jump detection
        assert!(OpCode::JMP.is_jump());
        assert!(OpCode::JMP_L.is_jump());
        assert!(OpCode::JMPIF.is_jump());
        assert!(OpCode::JMPIF_L.is_jump());
        assert!(!OpCode::ADD.is_jump());
        
        // Test call detection
        assert!(OpCode::CALL.is_call());
        assert!(OpCode::CALL_L.is_call());
        assert!(OpCode::CALLA.is_call());
        assert!(OpCode::SYSCALL.is_call());
        assert!(!OpCode::ADD.is_call());
        
        // Test terminator detection
        assert!(OpCode::RET.is_terminator());
        assert!(OpCode::JMP.is_terminator());
        assert!(OpCode::ABORT.is_terminator());
        assert!(!OpCode::ADD.is_terminator());
        
        // Test long form detection
        assert!(!OpCode::JMP.is_long_form());
        assert!(OpCode::JMP_L.is_long_form());
        assert!(OpCode::JMP.has_long_form());
        assert_eq!(OpCode::JMP.to_long_form(), OpCode::JMP_L);
    }

    #[test]
    fn test_stack_item_type_conversion() {
        let decoder = InstructionDecoder::new();
        
        // Test various stack item types
        let test_cases = vec![
            (0x00, StackItemType::Any),
            (0x10, StackItemType::Boolean),
            (0x21, StackItemType::Integer),
            (0x28, StackItemType::ByteString),
            (0x30, StackItemType::Buffer),
            (0x40, StackItemType::Array),
            (0x41, StackItemType::Struct),
            (0x48, StackItemType::Map),
            (0x60, StackItemType::InteropInterface),
            (0x70, StackItemType::Pointer),
        ];
        
        for (type_byte, expected_type) in test_cases {
            let bytecode = &[0xC2, type_byte]; // CONVERT with type
            let instruction = decoder.decode_instruction(bytecode, 0).unwrap();
            assert_eq!(instruction.opcode, OpCode::CONVERT);
            assert_eq!(instruction.operand, Some(Operand::StackItemType(expected_type)));
            assert_eq!(instruction.size, 2);
        }
        
        // Test invalid stack item type
        let bytecode = &[0xC2, 0xFF]; // CONVERT with invalid type
        let result = decoder.decode_instruction(bytecode, 0);
        assert!(matches!(result, Err(DisassemblyError::InvalidOperand { .. })));
    }
}
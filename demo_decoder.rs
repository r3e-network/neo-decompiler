#!/usr/bin/env cargo script

//! Demo showing the complete Neo N3 instruction decoder in action
//! 
//! This demonstrates the comprehensive instruction decoding capabilities
//! for all major Neo N3 opcode categories including:
//! - Integer push operations (various sizes)
//! - Data push operations (PUSHDATA variants)
//! - Control flow (jumps, calls, try-catch)
//! - Stack operations
//! - Slot operations
//! - Arithmetic and logical operations
//! - Array and string operations
//! - Type operations
//! - And more...

use neo_decompiler::core::disassembler::{Disassembler, InstructionDecoder};
use neo_decompiler::common::{types::*, config::DecompilerConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Neo N3 Instruction Decoder Demo");
    println!("================================\n");

    let config = DecompilerConfig::default();
    let disassembler = Disassembler::new(&config);
    let decoder = InstructionDecoder::new();

    // Demo bytecode representing a simple Neo N3 contract
    let demo_bytecode = vec![
        // Initialize slots: 3 locals, 2 statics
        0x57, 0x03, 0x02,           // INITSLOT 3, 2
        
        // Push integer constants
        0x00, 0x2A,                 // PUSHINT8 42
        0x69, 0x00,                 // STLOC 0
        
        0x01, 0x00, 0x01,           // PUSHINT16 256
        0x69, 0x01,                 // STLOC 1
        
        0x02, 0x00, 0x00, 0x10, 0x00, // PUSHINT32 1048576
        0x69, 0x02,                 // STLOC 2
        
        // Push data
        0x0C, 0x05, 0x48, 0x65, 0x6C, 0x6C, 0x6F, // PUSHDATA1 "Hello"
        
        // Stack operations
        0x4A,                       // DUP
        0x50,                       // SWAP
        
        // Arithmetic
        0x68, 0x00,                 // LDLOC 0
        0x68, 0x01,                 // LDLOC 1  
        0x8E,                       // ADD
        
        // Conditional jump (short form)
        0x24, 0x08,                 // JMPIF +8
        
        // Call syscall (example)
        0x41, 0x9B, 0xF5, 0x13, 0x41, // SYSCALL (System.Storage.GetContext)
        
        // Return
        0x40,                       // RET
        
        // Jump target: alternative path
        0x68, 0x02,                 // LDLOC 2
        0x40,                       // RET
    ];

    println!("Disassembling Neo N3 bytecode...\n");
    
    match disassembler.disassemble(&demo_bytecode) {
        Ok(instructions) => {
            println!("Successfully decoded {} instructions:\n", instructions.len());
            
            for (i, instruction) in instructions.iter().enumerate() {
                println!("{:3}: {:04X}  {:12} {}",
                    i,
                    instruction.offset,
                    format!("{:?}", instruction.opcode),
                    format_operand(&instruction.operand)
                );
            }
        }
        Err(e) => {
            eprintln!("Disassembly error: {}", e);
        }
    }

    println!("\n" + "=".repeat(50));
    println!("Individual Instruction Decoding Examples");
    println!("=".repeat(50) + "\n");

    // Demonstrate various instruction types
    let test_cases = vec![
        ("PUSHINT64 (max value)", vec![0x03, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F]),
        ("PUSHINT128", {
            let mut bytes = vec![0x04];
            bytes.extend(vec![0x01; 16]);
            bytes
        }),
        ("PUSHDATA2", vec![0x0D, 0x04, 0x00, 0x54, 0x65, 0x73, 0x74]),
        ("JMP_L (long form)", vec![0x23, 0x00, 0x01, 0x00, 0x00]),
        ("CALLA (method call)", vec![0x36, 0x12, 0x34]),
        ("TRY-CATCH", vec![0x3B, 0x0A, 0x14]),
        ("CONVERT to Integer", vec![0xC2, 0x21]),
        ("NEWARRAY (5 elements)", vec![0xAD, 0x05]),
        ("NEWBUFFER (1024 bytes)", vec![0x73, 0x00, 0x04]),
    ];

    for (description, bytecode) in test_cases {
        match decoder.decode_instruction(&bytecode, 0) {
            Ok(instruction) => {
                println!("{:25}: {} (size: {} bytes) {}",
                    description,
                    format!("{:?}", instruction.opcode),
                    instruction.size,
                    format_operand(&instruction.operand)
                );
            }
            Err(e) => {
                println!("{:25}: ERROR - {}", description, e);
            }
        }
    }

    println!("\n" + "=".repeat(50));
    println!("Opcode Properties Demo");
    println!("=".repeat(50) + "\n");

    let opcodes = vec![
        OpCode::JMP, OpCode::JMP_L, OpCode::JMPIF, OpCode::CALL, OpCode::CALLA, 
        OpCode::SYSCALL, OpCode::RET, OpCode::ABORT, OpCode::ADD, OpCode::CONVERT
    ];

    for opcode in opcodes {
        println!("{:12}: jump={}, call={}, terminator={}, long_form={}, has_long={}",
            format!("{:?}", opcode),
            opcode.is_jump(),
            opcode.is_call(),
            opcode.is_terminator(),
            opcode.is_long_form(),
            opcode.has_long_form()
        );
    }

    Ok(())
}

fn format_operand(operand: &Option<Operand>) -> String {
    match operand {
        None => String::new(),
        Some(op) => match op {
            Operand::Integer(val) => format!("({})", val),
            Operand::BigInteger(bytes) => format!("(BigInt: {} bytes)", bytes.len()),
            Operand::Bytes(bytes) => format!("(Data: {} bytes)", bytes.len()),
            Operand::JumpTarget8(target) => format!("(Jump: {:+})", target),
            Operand::JumpTarget32(target) => format!("(Jump: {:+})", target),
            Operand::SlotIndex(idx) => format!("(Slot: {})", idx),
            Operand::SyscallHash(hash) => format!("(Syscall: 0x{:08X})", hash),
            Operand::StackItemType(t) => format!("(Type: {:?})", t),
            Operand::TryBlock { catch_offset, finally_offset } => {
                format!("(Catch: {}, Finally: {:?})", catch_offset, finally_offset)
            },
            Operand::SlotInit { static_slots, local_slots } => {
                format!("(Static: {}, Local: {})", static_slots, local_slots)
            },
            Operand::MethodToken(token) => format!("(Method: 0x{:04X})", token),
            Operand::CallToken(token) => format!("(Call: 0x{:04X})", token),
            Operand::BufferSize(size) => format!("(Buffer: {} bytes)", size),
            Operand::Count(count) => format!("(Count: {})", count),
            Operand::Message(msg) => format!("(Msg: \"{}\")", msg),
        }
    }
}
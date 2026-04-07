#![no_main]

use libfuzzer_sys::fuzz_target;
use neo_decompiler::{Decompiler, NefParser};

// --- Opcode constants (from src/opcodes_generated.rs) ---

// Push opcodes (no operand)
const PUSHM1: u8 = 0x0F;
const PUSH0: u8 = 0x10;

// Push with operand
const PUSHINT8: u8 = 0x00; // I8 operand
const PUSHINT16: u8 = 0x01; // I16 operand
const PUSHT: u8 = 0x08; // no operand
const PUSHF: u8 = 0x09; // no operand
const PUSHNULL: u8 = 0x0B; // no operand

// Control flow
const NOP: u8 = 0x21;
const JMP: u8 = 0x22; // Jump8 operand
const JMPIF: u8 = 0x24; // Jump8 operand
const JMPIFNOT: u8 = 0x26; // Jump8 operand
const JMPNE: u8 = 0x2A; // Jump8 operand
const RET: u8 = 0x40;

// Stack manipulation (no operand)
const DROP: u8 = 0x45;
const DUP: u8 = 0x4A;
const OVER: u8 = 0x4B;
const SWAP: u8 = 0x50;
const ROT: u8 = 0x51;
const REVERSE3: u8 = 0x53;
const REVERSE4: u8 = 0x54;
const TUCK: u8 = 0x4E;
const NIP: u8 = 0x46;
const DEPTH: u8 = 0x43;

// Local variable slot management
const INITSLOT: u8 = 0x57; // Bytes(2) operand: [locals, args]
const LDLOC0: u8 = 0x68;
const STLOC0: u8 = 0x70;

// Arithmetic (no operand, binary: pop 2, push 1)
const ADD: u8 = 0x9E;
const SUB: u8 = 0x9F;
const MUL: u8 = 0xA0;
const DIV: u8 = 0xA1;
const MOD: u8 = 0xA2;
const SHL: u8 = 0xA8;
const SHR: u8 = 0xA9;

// Arithmetic (no operand, unary: pop 1, push 1)
const INC: u8 = 0x9C;
const DEC: u8 = 0x9D;
const NEGATE: u8 = 0x9B;
const ABS: u8 = 0x9A;
const SIGN: u8 = 0x99;
const NOT: u8 = 0xAA;
const NZ: u8 = 0xB1;
const INVERT: u8 = 0x90;

// Comparison (binary: pop 2, push 1)
const EQUAL: u8 = 0x97;
const NOTEQUAL: u8 = 0x98;
const LT: u8 = 0xB5;
const LE: u8 = 0xB6;
const GT: u8 = 0xB7;
const GE: u8 = 0xB8;
const MIN: u8 = 0xB9;
const MAX: u8 = 0xBA;
const NUMEQUAL: u8 = 0xB3;
const BOOLAND: u8 = 0xAB;
const BOOLOR: u8 = 0xAC;
// Bitwise (binary: pop 2, push 1)
const AND: u8 = 0x91;
const OR: u8 = 0x92;
const XOR: u8 = 0x93;

// Type / null checks (unary: pop 1, push 1)
const ISNULL: u8 = 0xD8;

// Collection opcodes
const NEWARRAY0: u8 = 0xC2; // push 1
const NEWSTRUCT0: u8 = 0xC5; // push 1
const NEWMAP: u8 = 0xC8; // push 1
const SIZE: u8 = 0xCA; // pop 1, push 1

/// Reads decision bytes from the fuzzer input, wrapping around when exhausted.
struct DecisionReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> DecisionReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn next(&mut self) -> u8 {
        if self.pos >= self.data.len() {
            self.pos = 0;
        }
        let v = self.data[self.pos];
        self.pos += 1;
        v
    }

}

/// Builds structurally valid NeoVM bytecode programs and wraps them in NEF.
struct ProgramBuilder {
    bytecode: Vec<u8>,
    stack_depth: i32,
    has_locals: bool,
    num_locals: u8,
}

impl ProgramBuilder {
    fn new() -> Self {
        Self {
            bytecode: Vec::new(),
            stack_depth: 0,
            has_locals: false,
            num_locals: 0,
        }
    }

    /// Emit INITSLOT at the start if we want locals/args.
    fn emit_initslot(&mut self, locals: u8, args: u8) {
        if self.has_locals || !self.bytecode.is_empty() {
            return; // INITSLOT must be first instruction
        }
        self.bytecode.push(INITSLOT);
        self.bytecode.push(locals);
        self.bytecode.push(args);
        self.has_locals = true;
        self.num_locals = locals;
    }

    /// Emit a push instruction (pushes 1 value onto the stack).
    fn emit_push(&mut self, r: &mut DecisionReader) {
        let choice = r.next() % 22;
        match choice {
            // PUSH0 through PUSH16 (no operand)
            0..=17 => {
                let opcode = if choice == 0 {
                    PUSHM1
                } else {
                    PUSH0 + (choice - 1)
                };
                self.bytecode.push(opcode);
            }
            // PUSHINT8 with 1-byte signed operand
            18 => {
                self.bytecode.push(PUSHINT8);
                self.bytecode.push(r.next());
            }
            // PUSHINT16 with 2-byte signed operand
            19 => {
                self.bytecode.push(PUSHINT16);
                self.bytecode.push(r.next());
                self.bytecode.push(r.next());
            }
            // PUSHT / PUSHF
            20 => self.bytecode.push(PUSHT),
            21 => self.bytecode.push(PUSHF),
            _ => unreachable!(),
        }
        self.stack_depth += 1;
    }

    /// Emit a binary arithmetic/comparison op (pops 2, pushes 1).
    fn emit_binary_op(&mut self, r: &mut DecisionReader) {
        if self.stack_depth < 2 {
            return;
        }
        const BINARY_OPS: [u8; 21] = [
            ADD, SUB, MUL, DIV, MOD, SHL, SHR, AND, OR, XOR, EQUAL, NOTEQUAL, LT, LE, GT, GE,
            MIN, MAX, NUMEQUAL, BOOLAND, BOOLOR,
        ];
        let idx = (r.next() as usize) % BINARY_OPS.len();
        self.bytecode.push(BINARY_OPS[idx]);
        self.stack_depth -= 1; // pop 2, push 1 => net -1
    }

    /// Emit a unary operation (pops 1, pushes 1 -- net 0).
    fn emit_unary_op(&mut self, r: &mut DecisionReader) {
        if self.stack_depth < 1 {
            return;
        }
        const UNARY_OPS: [u8; 10] = [INC, DEC, NEGATE, ABS, SIGN, NOT, NZ, INVERT, ISNULL, SIZE];
        let idx = (r.next() as usize) % UNARY_OPS.len();
        self.bytecode.push(UNARY_OPS[idx]);
        // stack_depth unchanged (pop 1, push 1)
    }

    /// Emit a stack manipulation instruction.
    fn emit_stack_op(&mut self, r: &mut DecisionReader) {
        let choice = r.next() % 8;
        match choice {
            0 if self.stack_depth >= 1 => {
                self.bytecode.push(DUP);
                self.stack_depth += 1;
            }
            1 if self.stack_depth >= 2 => {
                self.bytecode.push(SWAP);
                // depth unchanged
            }
            2 if self.stack_depth >= 2 => {
                self.bytecode.push(OVER);
                self.stack_depth += 1;
            }
            3 if self.stack_depth >= 3 => {
                self.bytecode.push(ROT);
                // depth unchanged
            }
            4 if self.stack_depth >= 3 => {
                self.bytecode.push(REVERSE3);
                // depth unchanged
            }
            5 if self.stack_depth >= 4 => {
                self.bytecode.push(REVERSE4);
                // depth unchanged
            }
            6 if self.stack_depth >= 2 => {
                self.bytecode.push(TUCK);
                self.stack_depth += 1;
            }
            7 if self.stack_depth >= 2 => {
                self.bytecode.push(NIP);
                self.stack_depth -= 1;
            }
            _ => {
                // Not enough stack depth for the chosen op; emit NOP instead.
                self.bytecode.push(NOP);
            }
        }
    }

    /// Emit a DROP instruction.
    fn emit_drop(&mut self) {
        if self.stack_depth < 1 {
            return;
        }
        self.bytecode.push(DROP);
        self.stack_depth -= 1;
    }

    /// Emit a local variable store (STLOC0/1/2) -- requires INITSLOT and stack >= 1.
    fn emit_stloc(&mut self, r: &mut DecisionReader) {
        if !self.has_locals || self.num_locals == 0 || self.stack_depth < 1 {
            return;
        }
        let slot = r.next() % self.num_locals.min(3);
        self.bytecode.push(STLOC0 + slot);
        self.stack_depth -= 1;
    }

    /// Emit a local variable load (LDLOC0/1/2) -- requires INITSLOT.
    fn emit_ldloc(&mut self, r: &mut DecisionReader) {
        if !self.has_locals || self.num_locals == 0 {
            return;
        }
        let slot = r.next() % self.num_locals.min(3);
        self.bytecode.push(LDLOC0 + slot);
        self.stack_depth += 1;
    }

    /// Emit a conditional or unconditional forward jump.
    /// We emit a JMP/JMPIF/JMPIFNOT with a small positive offset that skips
    /// over a few NOP instructions, so the jump target is always valid.
    fn emit_jump_block(&mut self, r: &mut DecisionReader) {
        let choice = r.next() % 4;
        let (opcode, needs_stack) = match choice {
            0 => (JMP, 0),
            1 => (JMPIF, 1),
            2 => (JMPIFNOT, 1),
            _ => (JMPNE, 2),
        };

        if self.stack_depth < needs_stack {
            return;
        }

        // Decide how many NOPs to skip over (1..=4).
        let nop_count = ((r.next() % 4) + 1) as i8;

        // The jump offset is relative to the opcode position.
        // JMP has Jump8 encoding: 1 byte opcode + 1 byte offset.
        // Offset = 2 (skip opcode+operand) + nop_count.
        let offset = 2 + nop_count;

        self.bytecode.push(opcode);
        self.bytecode.push(offset as u8);

        if needs_stack >= 2 {
            self.stack_depth -= 2; // JMPNE etc consume 2 values
        } else if needs_stack == 1 {
            self.stack_depth -= 1; // JMPIF/JMPIFNOT consume 1
        }
        // JMP consumes nothing

        // Emit the NOP padding that gets jumped over.
        for _ in 0..nop_count {
            self.bytecode.push(NOP);
        }
    }

    /// Emit a simple if-else block:
    ///   JMPIF +offset_to_else
    ///   <then-body: a few pushes>
    ///   JMP +offset_past_else
    ///   <else-body: a few pushes>
    fn emit_if_else(&mut self, r: &mut DecisionReader) {
        if self.stack_depth < 1 {
            return;
        }
        self.stack_depth -= 1; // JMPIF consumes the condition

        // We will fill in the jump offsets after we know body sizes.
        // Then-body: 1-2 push instructions.
        let then_push_count = ((r.next() % 2) + 1) as usize;
        // Else-body: 1-2 push instructions.
        let else_push_count = ((r.next() % 2) + 1) as usize;

        // Each push instruction (PUSH0..PUSH16) is 1 byte.
        // JMP instruction is 2 bytes (opcode + Jump8 offset).
        // JMPIFNOT instruction is 2 bytes.

        let then_body_size = then_push_count; // bytes
        let jmp_over_else_size: usize = 2; // JMP + offset
        let else_body_size = else_push_count; // bytes

        // JMPIFNOT offset: skip over then-body + JMP instruction
        // offset is relative to the JMPIFNOT opcode position
        // JMPIFNOT itself takes 2 bytes, so offset = 2 + then_body_size + jmp_over_else_size
        let jmpifnot_offset = (2 + then_body_size + jmp_over_else_size) as i8;

        // Emit JMPIFNOT to skip to else
        self.bytecode.push(JMPIFNOT);
        self.bytecode.push(jmpifnot_offset as u8);

        // Emit then-body
        let then_depth_before = self.stack_depth;
        for _ in 0..then_push_count {
            let push_val = PUSH0 + (r.next() % 17);
            self.bytecode.push(push_val);
            self.stack_depth += 1;
        }

        // JMP over else-body, offset relative to JMP opcode = 2 + else_body_size
        let jmp_offset = (2 + else_body_size) as i8;
        self.bytecode.push(JMP);
        self.bytecode.push(jmp_offset as u8);

        // Emit else-body (reset stack depth to match then path)
        self.stack_depth = then_depth_before;
        for _ in 0..else_push_count {
            let push_val = PUSH0 + (r.next() % 17);
            self.bytecode.push(push_val);
            self.stack_depth += 1;
        }
        // Both branches push the same count, so stack depth is consistent.
    }

    /// Emit DEPTH (pushes current stack depth as integer).
    fn emit_depth(&mut self) {
        self.bytecode.push(DEPTH);
        self.stack_depth += 1;
    }

    /// Emit PUSHNULL.
    fn emit_push_null(&mut self) {
        self.bytecode.push(PUSHNULL);
        self.stack_depth += 1;
    }

    /// Emit NEWARRAY0 / NEWSTRUCT0 / NEWMAP (push empty collection).
    fn emit_new_collection(&mut self, r: &mut DecisionReader) {
        let choice = r.next() % 3;
        match choice {
            0 => self.bytecode.push(NEWARRAY0),
            1 => self.bytecode.push(NEWSTRUCT0),
            _ => self.bytecode.push(NEWMAP),
        }
        self.stack_depth += 1;
    }

    /// Emit RET.
    fn emit_ret(&mut self) {
        self.bytecode.push(RET);
    }

    /// Wrap the bytecode in a valid NEF3 container with correct checksum.
    fn build_nef(&self) -> Vec<u8> {
        let mut nef = Vec::new();

        // Magic: "NEF3"
        nef.extend_from_slice(b"NEF3");

        // Compiler: 64-byte fixed field (null-padded)
        let mut compiler = [0u8; 64];
        let name = b"fuzz-grammar";
        compiler[..name.len()].copy_from_slice(name);
        nef.extend_from_slice(&compiler);

        // Source: empty varstring (varint 0)
        nef.push(0);

        // Reserved byte (must be 0)
        nef.push(0);

        // Method tokens: empty set (varint 0)
        nef.push(0);

        // Reserved word (must be 0)
        nef.extend_from_slice(&0u16.to_le_bytes());

        // Script as varbytes
        let script_len = self.bytecode.len() as u32;
        write_varint(&mut nef, script_len);
        nef.extend_from_slice(&self.bytecode);

        // Checksum: double-SHA256, first 4 bytes as LE u32
        let checksum = NefParser::calculate_checksum(&nef);
        nef.extend_from_slice(&checksum.to_le_bytes());

        nef
    }
}

fn write_varint(buf: &mut Vec<u8>, value: u32) {
    match value {
        0x00..=0xFC => buf.push(value as u8),
        0xFD..=0xFFFF => {
            buf.push(0xFD);
            buf.extend_from_slice(&(value as u16).to_le_bytes());
        }
        _ => {
            buf.push(0xFE);
            buf.extend_from_slice(&value.to_le_bytes());
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }

    let mut r = DecisionReader::new(data);
    let mut builder = ProgramBuilder::new();

    // Decide whether to emit INITSLOT (for local variable coverage).
    let use_locals = r.next() % 3 == 0;
    if use_locals {
        let num_locals = (r.next() % 3) + 1; // 1-3 locals
        builder.emit_initslot(num_locals, 0);
    }

    // Generate 5-50 instructions driven by fuzzer decisions.
    let num_instructions = (r.next() as usize % 46) + 5;
    for _ in 0..num_instructions {
        let choice = r.next() % 20;
        match choice {
            // Push values (most frequent -- feeds the stack)
            0..=5 => builder.emit_push(&mut r),
            // Binary arithmetic/comparison
            6..=8 => builder.emit_binary_op(&mut r),
            // Unary operations
            9..=10 => builder.emit_unary_op(&mut r),
            // Stack manipulation
            11..=12 => builder.emit_stack_op(&mut r),
            // Drop
            13 => builder.emit_drop(),
            // Local variable store
            14 => builder.emit_stloc(&mut r),
            // Local variable load
            15 => builder.emit_ldloc(&mut r),
            // Jump / conditional blocks
            16 => builder.emit_jump_block(&mut r),
            // If-else structured block
            17 => builder.emit_if_else(&mut r),
            // Depth / null / collections
            18 => {
                let sub = r.next() % 3;
                match sub {
                    0 => builder.emit_depth(),
                    1 => builder.emit_push_null(),
                    _ => builder.emit_new_collection(&mut r),
                }
            }
            // NOP (exercises NOP handling)
            _ => builder.bytecode.push(NOP),
        }
    }

    // Always terminate with RET for a well-formed program.
    builder.emit_ret();

    // Wrap in NEF and run the full decompilation pipeline.
    let nef = builder.build_nef();
    let decompiler = Decompiler::new();
    let _ = decompiler.decompile_bytes(&nef);
});

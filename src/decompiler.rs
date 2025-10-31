use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::disassembler::Disassembler;
use crate::error::Result;
use crate::instruction::{Instruction, OpCode, Operand, OperandEncoding};
use crate::manifest::ContractManifest;
use crate::native_contracts;
use crate::nef::{NefFile, NefParser};
use crate::util;

/// Main entry point used by the CLI and tests.
#[derive(Debug, Default)]
pub struct Decompiler {
    parser: NefParser,
    disassembler: Disassembler,
}

impl Decompiler {
    pub fn new() -> Self {
        Self {
            parser: NefParser::new(),
            disassembler: Disassembler::new(),
        }
    }

    /// Decompile a NEF blob already loaded in memory.
    pub fn decompile_bytes(&self, bytes: &[u8]) -> Result<Decompilation> {
        self.decompile_bytes_with_manifest(bytes, None)
    }

    /// Decompile a NEF blob using an optional manifest.
    pub fn decompile_bytes_with_manifest(
        &self,
        bytes: &[u8],
        manifest: Option<ContractManifest>,
    ) -> Result<Decompilation> {
        let nef = self.parser.parse(bytes)?;
        let instructions = self.disassembler.disassemble(&nef.script)?;
        let pseudocode = render_pseudocode(&instructions);
        let high_level = render_high_level(&nef, &instructions, manifest.as_ref());

        Ok(Decompilation {
            nef,
            manifest,
            instructions,
            pseudocode,
            high_level,
        })
    }

    /// Decompile a NEF file from disk.
    pub fn decompile_file<P: AsRef<Path>>(&self, path: P) -> Result<Decompilation> {
        let data = fs::read(path)?;
        self.decompile_bytes(&data)
    }

    /// Decompile a NEF file alongside an optional manifest file.
    pub fn decompile_file_with_manifest<P, Q>(
        &self,
        nef_path: P,
        manifest_path: Option<Q>,
    ) -> Result<Decompilation>
    where
        P: AsRef<Path>,
        Q: AsRef<Path>,
    {
        let data = fs::read(nef_path)?;
        let manifest = match manifest_path {
            Some(path) => Some(ContractManifest::from_file(path)?),
            None => None,
        };
        self.decompile_bytes_with_manifest(&data, manifest)
    }
}

/// Result of a successful decompilation run.
#[derive(Debug, Clone)]
pub struct Decompilation {
    pub nef: NefFile,
    pub manifest: Option<ContractManifest>,
    pub instructions: Vec<Instruction>,
    pub pseudocode: String,
    pub high_level: String,
}

fn render_pseudocode(instructions: &[Instruction]) -> String {
    use std::fmt::Write;

    let mut output = String::new();
    for instruction in instructions {
        let _ = write!(output, "{:04X}: {}", instruction.offset, instruction.opcode);
        if let Some(operand) = &instruction.operand {
            let _ = write!(output, " {}", operand);
        }
        output.push('\n');
    }
    output
}

fn render_high_level(
    nef: &NefFile,
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
) -> String {
    use std::fmt::Write;

    let mut output = String::new();
    let contract_name = manifest
        .and_then(|m| {
            let trimmed = m.name.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .unwrap_or("NeoContract");

    writeln!(output, "contract {contract_name} {{").unwrap();

    if let Some(manifest) = manifest {
        if !manifest.supported_standards.is_empty() {
            let standards = manifest
                .supported_standards
                .iter()
                .map(|s| format!("\"{s}\""))
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(output, "    supported_standards = [{standards}];").unwrap();
        }

        if manifest.features.storage || manifest.features.payable {
            writeln!(output, "    features {{").unwrap();
            if manifest.features.storage {
                writeln!(output, "        storage = true;").unwrap();
            }
            if manifest.features.payable {
                writeln!(output, "        payable = true;").unwrap();
            }
            writeln!(output, "    }}").unwrap();
        }

        if !manifest.abi.methods.is_empty() {
            writeln!(output, "    // ABI methods").unwrap();
            for method in &manifest.abi.methods {
                let params = method
                    .parameters
                    .iter()
                    .map(|param| format!("{}: {}", param.name, format_manifest_type(&param.kind)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let return_type = format_manifest_type(&method.return_type);
                let mut meta = Vec::new();
                if method.safe {
                    meta.push("safe".to_string());
                }
                if let Some(offset) = method.offset {
                    meta.push(format!("offset {}", offset));
                }
                let meta_comment = if meta.is_empty() {
                    String::new()
                } else {
                    format!(" // {}", meta.join(", "))
                };
                writeln!(
                    output,
                    "    fn {}({}) -> {};{}",
                    method.name, params, return_type, meta_comment
                )
                .unwrap();
            }
        }

        if !manifest.abi.events.is_empty() {
            writeln!(output, "    // ABI events").unwrap();
            for event in &manifest.abi.events {
                let params = event
                    .parameters
                    .iter()
                    .map(|param| format!("{}: {}", param.name, format_manifest_type(&param.kind)))
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(output, "    event {}({});", event.name, params).unwrap();
            }
        }
    } else {
        writeln!(output, "    // manifest not provided").unwrap();
    }

    if !nef.method_tokens.is_empty() {
        writeln!(output, "    // method tokens declared in NEF").unwrap();
        for token in &nef.method_tokens {
            let contract_note = native_contracts::lookup(&token.hash)
                .map(|info| format!(" ({})", info.name))
                .unwrap_or_default();
            writeln!(
                output,
                "    // {}{} hash={} params={} returns={} flags=0x{:02X}",
                token.method,
                contract_note,
                util::format_hash(&token.hash),
                token.parameters_count,
                token.has_return_value,
                token.call_flags
            )
            .unwrap();
        }
    }

    writeln!(output).unwrap();
    writeln!(output, "    fn script_entry() {{").unwrap();

    let mut emitter = HighLevelEmitter::with_program(instructions);
    for instruction in instructions {
        emitter.advance_to(instruction.offset);
        emitter.emit_instruction(instruction);
    }
    let statements = emitter.finish();

    if statements.is_empty() {
        writeln!(output, "        // no instructions decoded").unwrap();
    } else {
        for line in statements {
            if line.is_empty() {
                writeln!(output).unwrap();
            } else {
                writeln!(output, "        {line}").unwrap();
            }
        }
    }
    writeln!(output, "    }}").unwrap();
    writeln!(output, "}}").unwrap();

    output
}

fn format_manifest_type(kind: &str) -> String {
    match kind.to_ascii_lowercase().as_str() {
        "void" => "void".into(),
        "boolean" => "bool".into(),
        "integer" => "int".into(),
        "string" => "string".into(),
        "hash160" => "hash160".into(),
        "hash256" => "hash256".into(),
        "bytearray" => "bytes".into(),
        "signature" => "signature".into(),
        "array" => "array".into(),
        "map" => "map".into(),
        "interopinterface" => "interop".into(),
        "any" => "any".into(),
        other => other.to_string(),
    }
}

#[derive(Debug, Default)]
struct HighLevelEmitter {
    stack: Vec<String>,
    statements: Vec<String>,
    next_temp: usize,
    pending_closers: BTreeMap<usize, usize>,
    else_targets: BTreeMap<usize, usize>,
    skip_jumps: BTreeSet<usize>,
    program: Vec<Instruction>,
    index_by_offset: BTreeMap<usize, usize>,
}

impl HighLevelEmitter {
    fn with_program(instructions: &[Instruction]) -> Self {
        let mut emitter = Self {
            program: instructions.to_vec(),
            ..Self::default()
        };
        for (index, instruction) in instructions.iter().enumerate() {
            emitter.index_by_offset.insert(instruction.offset, index);
        }
        emitter
    }

    fn advance_to(&mut self, offset: usize) {
        if let Some(count) = self.pending_closers.remove(&offset) {
            for _ in 0..count {
                self.statements.push("}".into());
            }
        }

        if let Some(count) = self.else_targets.remove(&offset) {
            for _ in 0..count {
                self.statements.push("else {".into());
            }
        }
    }

    fn emit_instruction(&mut self, instruction: &Instruction) {
        use OpCode::*;

        match instruction.opcode {
            Pushint8 | Pushint16 | Pushint32 | Pushint64 | Pushint128 | Pushint256 | Pushdata1
            | Pushdata2 | Pushdata4 | PushM1 | Push0 | Push1 | Push2 | Push3 | Push4 | Push5
            | Push6 | Push7 | Push8 | Push9 | Push10 | Push11 | Push12 | Push13 | Push14
            | Push15 | Push16 | PushT | PushF | PushNull => {
                if let Some(operand) = &instruction.operand {
                    self.push_literal(instruction, operand.to_string());
                } else {
                    self.note(
                        instruction,
                        "literal push missing operand (malformed instruction)",
                    );
                }
            }
            Add => self.binary_op(instruction, "+"),
            Sub => self.binary_op(instruction, "-"),
            Mul => self.binary_op(instruction, "*"),
            Div => self.binary_op(instruction, "/"),
            Mod => self.binary_op(instruction, "%"),
            And => self.binary_op(instruction, "&"),
            Or => self.binary_op(instruction, "|"),
            Xor => self.binary_op(instruction, "^"),
            Shl => self.binary_op(instruction, "<<"),
            Shr => self.binary_op(instruction, ">>"),
            Equal | Numequal => self.binary_op(instruction, "=="),
            Notequal | Numnotequal => self.binary_op(instruction, "!="),
            Gt => self.binary_op(instruction, ">"),
            Ge => self.binary_op(instruction, ">="),
            Lt => self.binary_op(instruction, "<"),
            Le => self.binary_op(instruction, "<="),
            Booland => self.binary_op(instruction, "&&"),
            Boolor => self.binary_op(instruction, "||"),
            Inc => self.unary_op(instruction, |value| format!("{value} + 1")),
            Dec => self.unary_op(instruction, |value| format!("{value} - 1")),
            Negate => self.unary_op(instruction, |value| format!("-{value}")),
            Not => self.unary_op(instruction, |value| format!("!{value}")),
            Nz => self.unary_op(instruction, |value| format!("{value} != 0")),
            Abs => self.unary_op(instruction, |value| format!("{value}.abs()")),
            Drop => self.drop_top(instruction),
            Dup => self.dup_top(instruction),
            Over => self.over_second(instruction),
            Swap => self.swap_top(instruction),
            Nip => self.nip_second(instruction),
            Syscall => self.emit_syscall(instruction),
            Ret => self.emit_return(instruction),
            Jmp => self.emit_jump(instruction, 2),
            Jmp_L => self.emit_jump(instruction, 5),
            Jmpif => self.emit_relative(instruction, 2, "jump-if"),
            Jmpif_L => self.emit_relative(instruction, 5, "jump-if"),
            Jmpifnot => self.emit_if_block(instruction),
            Jmpifnot_L => self.emit_if_block(instruction),
            JmpEq => self.emit_relative(instruction, 2, "jump-if-eq"),
            JmpEq_L => self.emit_relative(instruction, 5, "jump-if-eq"),
            JmpNe => self.emit_relative(instruction, 2, "jump-if-ne"),
            JmpNe_L => self.emit_relative(instruction, 5, "jump-if-ne"),
            JmpGt => self.emit_relative(instruction, 2, "jump-if-gt"),
            JmpGt_L => self.emit_relative(instruction, 5, "jump-if-gt"),
            JmpGe => self.emit_relative(instruction, 2, "jump-if-ge"),
            JmpGe_L => self.emit_relative(instruction, 5, "jump-if-ge"),
            JmpLt => self.emit_relative(instruction, 2, "jump-if-lt"),
            JmpLt_L => self.emit_relative(instruction, 5, "jump-if-lt"),
            JmpLe => self.emit_relative(instruction, 2, "jump-if-le"),
            JmpLe_L => self.emit_relative(instruction, 5, "jump-if-le"),
            Endtry => self.emit_relative(instruction, 2, "end-try"),
            EndtryL => self.emit_relative(instruction, 5, "end-try"),
            Call => self.emit_relative(instruction, 2, "call"),
            Call_L => self.emit_relative(instruction, 5, "call"),
            CallA => self.emit_indirect_call(instruction, "calla"),
            CallT => self.emit_indirect_call(instruction, "callt"),
            Try | TryL => self.note(instruction, "try block (not yet lifted)"),
            Endfinally => self.note(instruction, "endfinally (not yet lifted)"),
            Nop => self.note(instruction, "noop"),
            _ => self.note(
                instruction,
                &format!("{} (not yet translated)", instruction.opcode.mnemonic()),
            ),
        }
    }

    fn finish(mut self) -> Vec<String> {
        if !self.pending_closers.is_empty() {
            let mut remaining: Vec<_> = self.pending_closers.into_iter().collect();
            remaining.sort_by_key(|(offset, _)| *offset);
            for (_, count) in remaining {
                for _ in 0..count {
                    self.statements.push("}".into());
                }
            }
        }
        self.statements
    }

    fn push_literal(&mut self, instruction: &Instruction, value: String) {
        self.push_comment(instruction);
        let temp = self.next_temp();
        self.statements.push(format!("let {temp} = {value};"));
        self.stack.push(temp);
    }

    fn binary_op(&mut self, instruction: &Instruction, symbol: &str) {
        self.push_comment(instruction);
        if self.stack.len() < 2 {
            self.stack_underflow(instruction, 2);
            return;
        }

        let right = self.stack.pop().unwrap();
        let left = self.stack.pop().unwrap();
        let temp = self.next_temp();
        self.statements
            .push(format!("let {temp} = {left} {symbol} {right};"));
        self.stack.push(temp);
    }

    fn unary_op<F>(&mut self, instruction: &Instruction, build: F)
    where
        F: Fn(&str) -> String,
    {
        self.push_comment(instruction);
        if let Some(value) = self.stack.pop() {
            let temp = self.next_temp();
            self.statements
                .push(format!("let {temp} = {};", build(&value)));
            self.stack.push(temp);
        } else {
            self.stack_underflow(instruction, 1);
        }
    }

    fn drop_top(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if let Some(value) = self.stack.pop() {
            self.statements.push(format!("// drop {value}"));
        } else {
            self.stack_underflow(instruction, 1);
        }
    }

    fn dup_top(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if let Some(value) = self.stack.last().cloned() {
            let temp = self.next_temp();
            self.statements
                .push(format!("let {temp} = {value}; // duplicate top of stack"));
            self.stack.push(temp);
        } else {
            self.stack_underflow(instruction, 1);
        }
    }

    fn over_second(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if self.stack.len() < 2 {
            self.stack_underflow(instruction, 2);
            return;
        }
        let value = self.stack[self.stack.len() - 2].clone();
        let temp = self.next_temp();
        self.statements
            .push(format!("let {temp} = {value}; // copy second stack value"));
        self.stack.push(temp);
    }

    fn swap_top(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if self.stack.len() < 2 {
            self.stack_underflow(instruction, 2);
            return;
        }
        let len = self.stack.len();
        self.stack.swap(len - 1, len - 2);
        self.statements
            .push("// swapped top two stack values".into());
    }

    fn nip_second(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if self.stack.len() < 2 {
            self.stack_underflow(instruction, 2);
            return;
        }
        let removed = self.stack.remove(self.stack.len() - 2);
        self.statements
            .push(format!("// remove second stack value {removed}"));
    }

    fn emit_return(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if let Some(value) = self.stack.pop() {
            self.statements.push(format!("return {value};"));
        } else {
            self.statements.push("return;".into());
        }
        self.stack.clear();
    }

    fn emit_syscall(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if let Some(Operand::Syscall(hash)) = instruction.operand {
            let temp = self.next_temp();
            if let Some(info) = crate::syscalls::lookup(hash) {
                self.statements.push(format!(
                    "let {temp} = syscall(\"{}\"); // 0x{hash:08X}",
                    info.name
                ));
            } else {
                self.statements
                    .push(format!("let {temp} = syscall(0x{hash:08X});"));
            }
            self.stack.push(temp);
        } else {
            self.statements.push(format!(
                "// {:04X}: missing syscall operand",
                instruction.offset
            ));
        }
    }

    fn emit_if_block(&mut self, instruction: &Instruction) {
        let width = Self::branch_width(instruction.opcode);
        let delta = match instruction.operand {
            Some(Operand::Jump(value)) => value as isize,
            Some(Operand::Jump32(value)) => value as isize,
            _ => {
                self.emit_relative(instruction, width, "jump-ifnot");
                return;
            }
        };
        let target = instruction.offset as isize + width + delta;
        if target <= instruction.offset as isize {
            self.emit_relative(instruction, width, "jump-ifnot");
            return;
        }
        let condition = match self.stack.pop() {
            Some(value) => value,
            None => {
                self.push_comment(instruction);
                self.stack_underflow(instruction, 1);
                return;
            }
        };
        self.push_comment(instruction);
        self.statements.push(format!("if {condition} {{"));
        let closer_entry = self.pending_closers.entry(target as usize).or_insert(0);
        *closer_entry += 1;

        if let Some((jump_offset, jump_target)) = self.detect_else(target as usize) {
            self.skip_jumps.insert(jump_offset);
            let else_entry = self.else_targets.entry(target as usize).or_insert(0);
            *else_entry += 1;
            let closer = self.pending_closers.entry(jump_target).or_insert(0);
            *closer += 1;
        }
    }

    fn emit_relative(&mut self, instruction: &Instruction, width: isize, label: &str) {
        let delta = match instruction.operand {
            Some(Operand::Jump(value)) => value as isize,
            Some(Operand::Jump32(value)) => value as isize,
            _ => {
                self.note(
                    instruction,
                    &format!("{label} with unsupported operand (skipping)"),
                );
                return;
            }
        };
        let target = instruction.offset as isize + width + delta;
        self.note(
            instruction,
            &format!("{label} -> 0x{target:04X} (control flow not yet lifted)"),
        );
    }

    fn emit_indirect_call(&mut self, instruction: &Instruction, label: &str) {
        let detail = match instruction.operand {
            Some(Operand::U16(value)) => format!("{label} 0x{value:04X}"),
            _ => format!("{label} (missing operand)"),
        };
        self.note(instruction, &format!("{detail} (not yet translated)"));
    }

    fn emit_jump(&mut self, instruction: &Instruction, width: isize) {
        if self.skip_jumps.remove(&instruction.offset) {
            // jump consumed by structured if/else handling
            return;
        }
        self.emit_relative(instruction, width, "jump");
    }

    fn note(&mut self, instruction: &Instruction, message: &str) {
        self.statements
            .push(format!("// {:04X}: {}", instruction.offset, message));
    }

    fn stack_underflow(&mut self, instruction: &Instruction, needed: usize) {
        self.statements.push(format!(
            "// {:04X}: insufficient values on stack for {} (needs {needed})",
            instruction.offset,
            instruction.opcode.mnemonic()
        ));
    }

    fn push_comment(&mut self, instruction: &Instruction) {
        self.statements.push(format!(
            "// {:04X}: {}",
            instruction.offset,
            instruction.opcode.mnemonic()
        ));
    }

    fn next_temp(&mut self) -> String {
        let name = format!("t{}", self.next_temp);
        self.next_temp += 1;
        name
    }

    fn detect_else(&self, false_offset: usize) -> Option<(usize, usize)> {
        let target_index = *self.index_by_offset.get(&false_offset)?;
        if target_index == 0 {
            return None;
        }
        let jump = self.program.get(target_index.checked_sub(1)?)?;
        let width = Self::branch_width(jump.opcode);
        let target = self.forward_jump_target(jump, width)?;
        if target > false_offset {
            Some((jump.offset, target))
        } else {
            None
        }
    }

    fn forward_jump_target(&self, instruction: &Instruction, width: isize) -> Option<usize> {
        let target = match instruction.operand {
            Some(Operand::Jump(delta)) => instruction.offset as isize + width + delta as isize,
            Some(Operand::Jump32(delta)) => instruction.offset as isize + width + delta as isize,
            _ => return None,
        };
        if target < 0 {
            return None;
        }
        Some(target as usize)
    }

    fn branch_width(opcode: OpCode) -> isize {
        match opcode.operand_encoding() {
            OperandEncoding::Jump8 => 2,
            OperandEncoding::Jump32 => 5,
            _ => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn build_nef(script: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"NEF3");
        let mut compiler = [0u8; 64];
        compiler[..4].copy_from_slice(b"test");
        data.extend_from_slice(&compiler);
        data.push(0); // source (empty)
        data.push(0); // reserved byte
        data.push(0); // method token count
        data.extend_from_slice(&0u16.to_le_bytes()); // reserved word
        write_varint(&mut data, script.len() as u32);
        data.extend_from_slice(script);
        let checksum = NefParser::calculate_checksum(&data);
        data.extend_from_slice(&checksum.to_le_bytes());
        data
    }

    fn sample_nef() -> Vec<u8> {
        // Build a minimal NEF with script: PUSH0, PUSH1, ADD, RET
        build_nef(&[0x10, 0x11, 0x9E, 0x40])
    }

    fn sample_manifest() -> ContractManifest {
        ContractManifest::from_json_str(
            r#"
            {
                "name": "ExampleContract",
                "supportedstandards": ["NEP-17"],
                "features": {"storage": true, "payable": false},
                "abi": {
                    "methods": [
                        {
                            "name": "main",
                            "parameters": [],
                            "returntype": "Integer",
                            "offset": 0,
                            "safe": false
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*"
            }
            "#,
        )
        .expect("manifest parsed")
    }

    #[test]
    fn decompile_end_to_end() {
        let nef_bytes = sample_nef();
        let decompilation = Decompiler::new()
            .decompile_bytes(&nef_bytes)
            .expect("decompile succeeds");

        assert_eq!(decompilation.instructions.len(), 4);
        assert!(decompilation.pseudocode.contains("ADD"));
        assert!(decompilation.high_level.contains("contract NeoContract"));
        assert!(decompilation.high_level.contains("fn script_entry()"));
    }

    #[test]
    fn decompile_with_manifest_produces_contract_name() {
        let nef_bytes = sample_nef();
        let manifest = sample_manifest();
        let decompilation = Decompiler::new()
            .decompile_bytes_with_manifest(&nef_bytes, Some(manifest))
            .expect("decompile succeeds with manifest");

        assert!(decompilation
            .high_level
            .contains("contract ExampleContract"));
        assert!(decompilation.high_level.contains("fn main() -> int;"));
    }

    #[test]
    fn decompile_syscall_includes_human_name() {
        // Script: SYSCALL(System.Runtime.Platform) ; RET
        let script = [0x41, 0xB2, 0x79, 0xFC, 0xF6, 0x40];
        let nef_bytes = build_nef(&script);
        let decompilation = Decompiler::new()
            .decompile_bytes(&nef_bytes)
            .expect("decompile succeeds");

        assert!(decompilation.pseudocode.contains("System.Runtime.Platform"));
        assert!(decompilation
            .high_level
            .contains("syscall(\"System.Runtime.Platform\")"));
    }

    #[test]
    fn high_level_lifts_boolean_ops() {
        // Script: PUSH1, PUSH1, BOOLAND, RET
        let script = [0x11, 0x11, 0xAB, 0x40];
        let nef_bytes = build_nef(&script);
        let decompilation = Decompiler::new()
            .decompile_bytes(&nef_bytes)
            .expect("decompile succeeds");

        assert!(decompilation.high_level.contains("let t2 = t0 && t1;"));
    }

    #[test]
    fn high_level_handles_stack_manipulation_and_unary_ops() {
        // Script: PUSH1, DUP, ADD, INC, RET
        let script = [0x11, 0x4A, 0x9E, 0x9C, 0x40];
        let nef_bytes = build_nef(&script);
        let decompilation = Decompiler::new()
            .decompile_bytes(&nef_bytes)
            .expect("decompile succeeds");

        assert!(decompilation
            .high_level
            .contains("let t1 = t0; // duplicate top of stack"));
        assert!(decompilation.high_level.contains("let t3 = t2 + 1;"));
    }

    #[test]
    fn high_level_lifts_simple_if_block() {
        // Script: PUSH1, JMPIFNOT +3, PUSH2, RET, PUSH3, RET
        let script = [0x11, 0x26, 0x03, 0x12, 0x40, 0x13, 0x40];
        let nef_bytes = build_nef(&script);
        let decompilation = Decompiler::new()
            .decompile_bytes(&nef_bytes)
            .expect("decompile succeeds");

        let high_level = &decompilation.high_level;
        assert!(high_level.contains("if t0 {"));
        assert!(high_level.contains("// 0003: PUSH2"));
        assert!(high_level.contains("}\n        // 0006: RET"));
    }

    #[test]
    fn high_level_closes_if_at_end() {
        // Script: PUSH1, JMPIFNOT +2, PUSH2, RET
        let script = [0x11, 0x26, 0x02, 0x12, 0x40];
        let nef_bytes = build_nef(&script);
        let decompilation = Decompiler::new()
            .decompile_bytes(&nef_bytes)
            .expect("decompile succeeds");

        let high_level = &decompilation.high_level;
        assert!(high_level.contains("if t0 {"));
        assert!(high_level.contains("        }\n    }\n}"));
    }

    #[test]
    fn high_level_lifts_if_else_block() {
        // Script: PUSH1, JMPIFNOT +3, PUSH2, JMP +2, PUSH3, RET, RET
        let script = [0x11, 0x26, 0x03, 0x12, 0x22, 0x02, 0x13, 0x40, 0x40];
        let nef_bytes = build_nef(&script);
        let decompilation = Decompiler::new()
            .decompile_bytes(&nef_bytes)
            .expect("decompile succeeds");

        let high_level = &decompilation.high_level;
        assert!(high_level.contains("if t0 {"));
        assert!(high_level.contains("else {"));
        assert!(high_level.contains("let t1 = 2;"));
        assert!(high_level.contains("let t2 = 3;"));
    }
}

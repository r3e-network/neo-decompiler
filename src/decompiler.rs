use std::fs;
use std::path::Path;

use crate::disassembler::Disassembler;
use crate::error::Result;
use crate::instruction::{Instruction, OpCode, Operand};
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
                "    // {}{} hash={} params={} return=0x{:02X} flags=0x{:02X}",
                token.method,
                contract_note,
                util::format_hash(&token.hash),
                token.params,
                token.return_type,
                token.call_flags
            )
            .unwrap();
        }
    }

    writeln!(output).unwrap();
    writeln!(output, "    fn script_entry() {{").unwrap();

    let mut emitter = HighLevelEmitter::new();
    for instruction in instructions {
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
}

impl HighLevelEmitter {
    fn new() -> Self {
        Self::default()
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
            Syscall => self.emit_syscall(instruction),
            Ret => self.emit_return(instruction),
            Jmp => self.emit_relative(instruction, 2, "jump"),
            Jmp_L => self.emit_relative(instruction, 5, "jump"),
            Jmpif => self.emit_relative(instruction, 2, "jump-if"),
            Jmpif_L => self.emit_relative(instruction, 5, "jump-if"),
            Jmpifnot => self.emit_relative(instruction, 2, "jump-ifnot"),
            Jmpifnot_L => self.emit_relative(instruction, 5, "jump-ifnot"),
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

    fn finish(self) -> Vec<String> {
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
            self.statements.push(format!(
                "// {:04X}: insufficient values on stack for {}",
                instruction.offset,
                instruction.opcode.mnemonic()
            ));
            return;
        }

        let right = self.stack.pop().unwrap();
        let left = self.stack.pop().unwrap();
        let temp = self.next_temp();
        self.statements
            .push(format!("let {temp} = {left} {symbol} {right};"));
        self.stack.push(temp);
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

    fn note(&mut self, instruction: &Instruction, message: &str) {
        self.statements
            .push(format!("// {:04X}: {}", instruction.offset, message));
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_nef(script: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"NEF3");
        let mut compiler = [0u8; 32];
        compiler[..4].copy_from_slice(b"test");
        data.extend_from_slice(&compiler);
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&(script.len() as u32).to_le_bytes());
        data.push(0); // method token count
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
}

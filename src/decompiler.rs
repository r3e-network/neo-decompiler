use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::disassembler::Disassembler;
use crate::error::Result;
use crate::instruction::{Instruction, OpCode, Operand, OperandEncoding};
use crate::manifest::{ContractManifest, ManifestMethod, ManifestParameter, ManifestPermission};
use crate::native_contracts;
use crate::nef::{describe_call_flags, NefFile, NefParser};
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
        let csharp = render_csharp(&nef, &instructions, manifest.as_ref());

        Ok(Decompilation {
            nef,
            manifest,
            instructions,
            pseudocode,
            high_level,
            csharp,
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
    pub csharp: String,
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
    let script_hash = nef.script_hash();
    writeln!(
        output,
        "    // script hash (little-endian): {}",
        util::format_hash(&script_hash)
    )
    .unwrap();
    writeln!(
        output,
        "    // script hash (big-endian): {}",
        util::format_hash_be(&script_hash)
    )
    .unwrap();

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

        if !manifest.permissions.is_empty() {
            writeln!(output, "    permissions {{").unwrap();
            for permission in &manifest.permissions {
                writeln!(output, "        {}", format_permission_entry(permission)).unwrap();
            }
            writeln!(output, "    }}").unwrap();
        }

        if let Some(trusts) = manifest.trusts.as_ref() {
            writeln!(output, "    trusts = {};", trusts.describe()).unwrap();
        }
        if let Some(author) = manifest_extra_string(manifest, "author") {
            writeln!(output, "    // author: {author}").unwrap();
        }
        if let Some(email) = manifest_extra_string(manifest, "email") {
            writeln!(output, "    // email: {email}").unwrap();
        }

        if !manifest.abi.methods.is_empty() {
            writeln!(output, "    // ABI methods").unwrap();
        for method in &manifest.abi.methods {
            let params = format_manifest_parameters(&method.parameters);
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
            let hint = native_contracts::describe_method_token(&token.hash, &token.method);
            let contract_note = hint
                .as_ref()
                .map(|h| format!(" ({})", h.formatted_label(&token.method)))
                .unwrap_or_default();
            writeln!(
                output,
                "    // {}{} hash={} params={} returns={} flags=0x{:02X} ({})",
                token.method,
                contract_note,
                util::format_hash(&token.hash),
                token.parameters_count,
                token.has_return_value,
                token.call_flags,
                describe_call_flags(token.call_flags)
            )
            .unwrap();
            if let Some(hint) = hint {
                if !hint.has_exact_method() {
                    writeln!(
                        output,
                        "    // warning: native contract {} does not expose method {}",
                        hint.contract, token.method
                    )
                    .unwrap();
                }
            }
        }
    }

    writeln!(output).unwrap();
    let entry_offset = instructions.first().map(|ins| ins.offset).unwrap_or(0);
    let entry_method = manifest.and_then(|m| find_manifest_entry_method(m, entry_offset));
    let entry_param_labels = entry_method.as_ref().map(|method| {
        method
            .parameters
            .iter()
            .map(|param| sanitize_identifier(&param.name))
            .collect::<Vec<_>>()
    });
    let entry_name = entry_method
        .as_ref()
        .map(|method| method.name.as_str())
        .unwrap_or("script_entry");
    let entry_params = entry_method
        .as_ref()
        .map(|method| format_manifest_parameters(&method.parameters))
        .unwrap_or_default();
    let entry_return = entry_method
        .as_ref()
        .map(|method| format_manifest_type(&method.return_type))
        .filter(|ty| ty != "void");
    let signature = match entry_return {
        Some(ret) => format!("fn {entry_name}({entry_params}) -> {ret}"),
        None => format!("fn {entry_name}({entry_params})"),
    };
    writeln!(output, "    {signature} {{").unwrap();

    let mut emitter = HighLevelEmitter::with_program(instructions);
    if let Some(labels) = entry_param_labels.as_ref() {
        emitter.set_argument_labels(labels);
    }
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

fn render_csharp(
    nef: &NefFile,
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
) -> String {
    use std::fmt::Write;

    let mut output = String::new();
    writeln!(output, "using System;").unwrap();
    writeln!(output, "using System.Numerics;").unwrap();
    writeln!(output, "using Neo.SmartContract.Framework;").unwrap();
    writeln!(output, "using Neo.SmartContract.Framework.Attributes;").unwrap();
    writeln!(output, "using Neo.SmartContract.Framework.Services;").unwrap();
    writeln!(output).unwrap();

    let contract_name = manifest
        .and_then(|m| {
            let trimmed = m.name.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .map(|name| sanitize_identifier(name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "NeoContract".to_string());

    writeln!(output, "namespace NeoDecompiler.Generated {{").unwrap();
    if let Some(manifest) = manifest {
        if let Some(author) = manifest_extra_string(manifest, "author") {
            writeln!(
                output,
                "    [ManifestExtra(\"Author\", \"{}\")]",
                escape_csharp_string(&author)
            )
            .unwrap();
        }
        if let Some(email) = manifest_extra_string(manifest, "email") {
            writeln!(
                output,
                "    [ManifestExtra(\"Email\", \"{}\")]",
                escape_csharp_string(&email)
            )
            .unwrap();
        }
    }
    writeln!(output, "    public class {contract_name} : SmartContract").unwrap();
    writeln!(output, "    {{").unwrap();
    let script_hash = nef.script_hash();
    writeln!(
        output,
        "        // script hash (little-endian): {}",
        util::format_hash(&script_hash)
    )
    .unwrap();
    writeln!(
        output,
        "        // script hash (big-endian): {}",
        util::format_hash_be(&script_hash)
    )
    .unwrap();

    if let Some(manifest) = manifest {
        if !manifest.supported_standards.is_empty() {
            let standards = manifest.supported_standards.join(", ");
            writeln!(output, "        // supported standards: {standards}").unwrap();
        }
        if manifest.features.storage || manifest.features.payable {
            writeln!(output, "        // features:").unwrap();
            if manifest.features.storage {
                writeln!(output, "        //   storage = true").unwrap();
            }
            if manifest.features.payable {
                writeln!(output, "        //   payable = true").unwrap();
            }
        }
        if !manifest.permissions.is_empty() {
            writeln!(output, "        // permissions:").unwrap();
            for permission in &manifest.permissions {
                writeln!(
                    output,
                    "        //   {}",
                    format_permission_entry(permission)
                )
                .unwrap();
            }
        }
        if let Some(trusts) = manifest.trusts.as_ref() {
            writeln!(output, "        // trusts = {}", trusts.describe()).unwrap();
        }
    } else {
        writeln!(output, "        // manifest not provided").unwrap();
    }

    writeln!(output).unwrap();

    let entry_offset = instructions.first().map(|ins| ins.offset).unwrap_or(0);
    let entry_method = manifest.and_then(|m| find_manifest_entry_method(m, entry_offset));
    let entry_parameters = entry_method
        .as_ref()
        .map(|method| collect_csharp_parameters(&method.parameters));
    let entry_param_labels = entry_parameters.as_ref().map(|params| {
        params.iter().map(|param| param.name.clone()).collect::<Vec<_>>()
    });
    let entry_method_name = entry_method
        .as_ref()
        .map(|method| sanitize_identifier(&method.name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "ScriptEntry".to_string());
    let entry_return = entry_method
        .as_ref()
        .map(|method| format_manifest_type_csharp(&method.return_type))
        .unwrap_or_else(|| "void".to_string());
    let entry_param_signature = entry_parameters
        .as_ref()
        .map(|params| format_csharp_parameters(params))
        .unwrap_or_default();
    let entry_signature = format_method_signature(&entry_method_name, &entry_param_signature, &entry_return);

    writeln!(output, "        {entry_signature}").unwrap();
    writeln!(output, "        {{").unwrap();

    let mut emitter = HighLevelEmitter::with_program(instructions);
    if let Some(labels) = entry_param_labels.as_ref() {
        emitter.set_argument_labels(labels);
    }
    for instruction in instructions {
        emitter.advance_to(instruction.offset);
        emitter.emit_instruction(instruction);
    }
    let statements = emitter.finish();
    if statements.is_empty() {
        writeln!(output, "            // no instructions decoded").unwrap();
    } else {
        for line in statements {
            let converted = csharpize_statement(&line);
            if converted.is_empty() {
                writeln!(output).unwrap();
            } else {
                writeln!(output, "            {converted}").unwrap();
            }
        }
    }
    writeln!(output, "        }}").unwrap();

    if let Some(manifest) = manifest {
        for method in &manifest.abi.methods {
            let is_entry = entry_method.as_ref().map_or(false, |entry| {
                entry.name == method.name && entry.offset == method.offset
            });
            if is_entry {
                continue;
            }
            let params = collect_csharp_parameters(&method.parameters);
            let param_signature = format_csharp_parameters(&params);
            let method_name = sanitize_identifier(&method.name);
            let return_type = format_manifest_type_csharp(&method.return_type);
            let signature = format_method_signature(&method_name, &param_signature, &return_type);
            writeln!(output).unwrap();
            writeln!(output, "        {signature}").unwrap();
            writeln!(output, "        {{").unwrap();
            writeln!(
                output,
                "            throw new System.NotImplementedException();"
            )
            .unwrap();
            writeln!(output, "        }}").unwrap();
        }
    }

    writeln!(output, "    }}").unwrap();
    writeln!(output, "}}").unwrap();

    output
}

fn sanitize_identifier(input: &str) -> String {
    let mut ident = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            ident.push(ch);
        } else if ch == '_' {
            ident.push('_');
        } else if ch.is_whitespace() || ch == '-' {
            if !ident.ends_with('_') {
                ident.push('_');
            }
        }
    }
    while ident.ends_with('_') {
        ident.pop();
    }
    if ident.is_empty() {
        ident.push_str("param");
    }
    if ident.chars().next().map(|ch| ch.is_ascii_digit()).unwrap_or(false) {
        ident.insert(0, '_');
    }
    ident
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

#[derive(Clone)]
struct CSharpParameter {
    name: String,
    ty: String,
}

fn collect_csharp_parameters(parameters: &[ManifestParameter]) -> Vec<CSharpParameter> {
    parameters
        .iter()
        .map(|param| CSharpParameter {
            name: sanitize_identifier(&param.name),
            ty: format_manifest_type_csharp(&param.kind),
        })
        .collect()
}

fn format_csharp_parameters(params: &[CSharpParameter]) -> String {
    params
        .iter()
        .map(|param| format!("{} {}", param.ty, param.name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_manifest_type_csharp(kind: &str) -> String {
    match kind.to_ascii_lowercase().as_str() {
        "void" => "void".into(),
        "boolean" | "bool" => "bool".into(),
        "integer" | "int" => "BigInteger".into(),
        "string" => "string".into(),
        "hash160" => "UInt160".into(),
        "hash256" => "UInt256".into(),
        "bytearray" | "bytes" => "ByteString".into(),
        "signature" => "ByteString".into(),
        "array" => "object[]".into(),
        "map" => "object".into(),
        "interopinterface" => "object".into(),
        "any" => "object".into(),
        _ => "object".into(),
    }
}

fn format_method_signature(name: &str, parameters: &str, return_type: &str) -> String {
    if parameters.is_empty() {
        format!("public static {return_type} {name}()")
    } else {
        format!("public static {return_type} {name}({parameters})")
    }
}

fn csharpize_statement(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("//") {
        return trimmed.to_string();
    }
    if trimmed.starts_with("let ") {
        return format!("var {}", &trimmed[4..]);
    }
    if trimmed.starts_with("if ") && trimmed.ends_with(" {") {
        let condition = trimmed[3..trimmed.len() - 2].trim();
        return format!("if ({condition}) {{");
    }
    if trimmed.starts_with("while ") && trimmed.ends_with(" {") {
        let condition = trimmed[6..trimmed.len() - 2].trim();
        return format!("while ({condition}) {{");
    }
    if trimmed.starts_with("for (") && trimmed.ends_with(" {") {
        let inner = &trimmed[4..trimmed.len() - 2];
        let converted = inner.replacen("let ", "var ", 1);
        return format!("for ({converted}) {{");
    }
    trimmed.to_string()
}

fn escape_csharp_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn format_manifest_parameters(parameters: &[ManifestParameter]) -> String {
    parameters
        .iter()
        .map(|param| format!("{}: {}", param.name, format_manifest_type(&param.kind)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn manifest_extra_string(manifest: &ContractManifest, key: &str) -> Option<String> {
    let extra = manifest.extra.as_ref()?;
    let map = match extra {
        serde_json::Value::Object(map) => map,
        _ => return None,
    };
    let target = key.to_ascii_lowercase();
    map.iter()
        .find(|(candidate, _)| candidate.to_ascii_lowercase() == target)
        .and_then(|(_, value)| value.as_str().map(|s| s.to_string()))
}

fn find_manifest_entry_method<'a>(
    manifest: &'a ContractManifest,
    entry_offset: usize,
) -> Option<&'a ManifestMethod> {
    manifest
        .abi
        .methods
        .iter()
        .find(|method| method.offset.map(|value| value as usize) == Some(entry_offset))
}

fn format_permission_entry(permission: &ManifestPermission) -> String {
    format!(
        "contract={} methods={}",
        permission.contract.describe(),
        permission.methods.describe()
    )
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
    do_while_headers: BTreeMap<usize, Vec<DoWhileLoop>>,
    active_do_while_tails: BTreeSet<usize>,
    loop_stack: Vec<LoopContext>,
    initialized_locals: BTreeSet<usize>,
    initialized_statics: BTreeSet<usize>,
    argument_labels: BTreeMap<usize, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotKind {
    Local,
    Argument,
    Static,
}

#[derive(Clone, Debug)]
struct LoopContext {
    break_offset: usize,
    continue_offset: usize,
}

#[derive(Clone, Debug)]
struct DoWhileLoop {
    tail_offset: usize,
    break_offset: usize,
}

#[derive(Clone, Debug)]
struct LoopJump {
    jump_offset: usize,
    target: usize,
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
        emitter.analyze_do_while_loops();
        emitter
    }

    fn set_argument_labels(&mut self, labels: &[String]) {
        for (index, label) in labels.iter().enumerate() {
            self.argument_labels.insert(index, label.clone());
        }
    }

    fn advance_to(&mut self, offset: usize) {
        if let Some(count) = self.pending_closers.remove(&offset) {
            for _ in 0..count {
                self.statements.push("}".into());
            }
        }

        self.close_loops_at(offset);

        if let Some(count) = self.else_targets.remove(&offset) {
            for _ in 0..count {
                self.statements.push("else {".into());
            }
        }

        if let Some(entries) = self.do_while_headers.remove(&offset) {
            for entry in entries {
                self.statements.push("do {".into());
                self.active_do_while_tails.insert(entry.tail_offset);
                self.loop_stack.push(LoopContext {
                    break_offset: entry.break_offset,
                    continue_offset: entry.tail_offset,
                });
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
            Jmpif => {
                if !self.try_emit_do_while_tail(instruction) {
                    self.emit_relative(instruction, 2, "jump-if");
                }
            }
            Jmpif_L => {
                if !self.try_emit_do_while_tail(instruction) {
                    self.emit_relative(instruction, 5, "jump-if");
                }
            }
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
            Initsslot => self.emit_init_static_slots(instruction),
            Initslot => self.emit_init_slots(instruction),
            Ldsfld0 => self.emit_load_slot(instruction, SlotKind::Static, 0),
            Ldsfld1 => self.emit_load_slot(instruction, SlotKind::Static, 1),
            Ldsfld2 => self.emit_load_slot(instruction, SlotKind::Static, 2),
            Ldsfld3 => self.emit_load_slot(instruction, SlotKind::Static, 3),
            Ldsfld4 => self.emit_load_slot(instruction, SlotKind::Static, 4),
            Ldsfld5 => self.emit_load_slot(instruction, SlotKind::Static, 5),
            Ldsfld6 => self.emit_load_slot(instruction, SlotKind::Static, 6),
            Ldsfld => self.emit_load_slot_from_operand(instruction, SlotKind::Static),
            Stsfld0 => self.emit_store_slot(instruction, SlotKind::Static, 0),
            Stsfld1 => self.emit_store_slot(instruction, SlotKind::Static, 1),
            Stsfld2 => self.emit_store_slot(instruction, SlotKind::Static, 2),
            Stsfld3 => self.emit_store_slot(instruction, SlotKind::Static, 3),
            Stsfld4 => self.emit_store_slot(instruction, SlotKind::Static, 4),
            Stsfld5 => self.emit_store_slot(instruction, SlotKind::Static, 5),
            Stsfld6 => self.emit_store_slot(instruction, SlotKind::Static, 6),
            Stsfld => self.emit_store_slot_from_operand(instruction, SlotKind::Static),
            Ldloc0 => self.emit_load_slot(instruction, SlotKind::Local, 0),
            Ldloc1 => self.emit_load_slot(instruction, SlotKind::Local, 1),
            Ldloc2 => self.emit_load_slot(instruction, SlotKind::Local, 2),
            Ldloc3 => self.emit_load_slot(instruction, SlotKind::Local, 3),
            Ldloc4 => self.emit_load_slot(instruction, SlotKind::Local, 4),
            Ldloc5 => self.emit_load_slot(instruction, SlotKind::Local, 5),
            Ldloc6 => self.emit_load_slot(instruction, SlotKind::Local, 6),
            Ldloc => self.emit_load_slot_from_operand(instruction, SlotKind::Local),
            Stloc0 => self.emit_store_slot(instruction, SlotKind::Local, 0),
            Stloc1 => self.emit_store_slot(instruction, SlotKind::Local, 1),
            Stloc2 => self.emit_store_slot(instruction, SlotKind::Local, 2),
            Stloc3 => self.emit_store_slot(instruction, SlotKind::Local, 3),
            Stloc4 => self.emit_store_slot(instruction, SlotKind::Local, 4),
            Stloc5 => self.emit_store_slot(instruction, SlotKind::Local, 5),
            Stloc6 => self.emit_store_slot(instruction, SlotKind::Local, 6),
            Stloc => self.emit_store_slot_from_operand(instruction, SlotKind::Local),
            Ldarg0 => self.emit_load_slot(instruction, SlotKind::Argument, 0),
            Ldarg1 => self.emit_load_slot(instruction, SlotKind::Argument, 1),
            Ldarg2 => self.emit_load_slot(instruction, SlotKind::Argument, 2),
            Ldarg3 => self.emit_load_slot(instruction, SlotKind::Argument, 3),
            Ldarg4 => self.emit_load_slot(instruction, SlotKind::Argument, 4),
            Ldarg5 => self.emit_load_slot(instruction, SlotKind::Argument, 5),
            Ldarg6 => self.emit_load_slot(instruction, SlotKind::Argument, 6),
            Ldarg => self.emit_load_slot_from_operand(instruction, SlotKind::Argument),
            Starg0 => self.emit_store_slot(instruction, SlotKind::Argument, 0),
            Starg1 => self.emit_store_slot(instruction, SlotKind::Argument, 1),
            Starg2 => self.emit_store_slot(instruction, SlotKind::Argument, 2),
            Starg3 => self.emit_store_slot(instruction, SlotKind::Argument, 3),
            Starg4 => self.emit_store_slot(instruction, SlotKind::Argument, 4),
            Starg5 => self.emit_store_slot(instruction, SlotKind::Argument, 5),
            Starg6 => self.emit_store_slot(instruction, SlotKind::Argument, 6),
            Starg => self.emit_store_slot_from_operand(instruction, SlotKind::Argument),
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
        Self::rewrite_for_loops(&mut self.statements);
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

    fn emit_init_static_slots(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        match instruction.operand {
            Some(Operand::U8(count)) => {
                self.statements
                    .push(format!("// declare {count} static slots"));
            }
            _ => self
                .statements
                .push("// missing INITSSLOT operand".into()),
        }
    }

    fn emit_init_slots(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        match &instruction.operand {
            Some(Operand::Bytes(bytes)) if bytes.len() >= 2 => {
                let locals = bytes[0];
                let args = bytes[1];
                self.statements.push(format!(
                    "// declare {locals} locals, {args} arguments"
                ));
            }
            _ => self
                .statements
                .push("// missing INITSLOT operand".into()),
        }
    }

    fn emit_load_slot(&mut self, instruction: &Instruction, kind: SlotKind, index: usize) {
        self.push_comment(instruction);
        let name = self.slot_label(kind, index);
        self.stack.push(name);
    }

    fn emit_load_slot_from_operand(&mut self, instruction: &Instruction, kind: SlotKind) {
        let Some(index) = Self::slot_index_from_operand(instruction) else {
            self.note(
                instruction,
                &format!("{} missing operand", instruction.opcode.mnemonic()),
            );
            return;
        };
        self.emit_load_slot(instruction, kind, index);
    }

    fn emit_store_slot(&mut self, instruction: &Instruction, kind: SlotKind, index: usize) {
        self.push_comment(instruction);
        if let Some(value) = self.stack.pop() {
            let name = self.slot_label(kind, index);
            let use_let = match kind {
                SlotKind::Local => self.initialized_locals.insert(index),
                SlotKind::Static => self.initialized_statics.insert(index),
                SlotKind::Argument => false,
            };
            if use_let {
                self.statements.push(format!("let {name} = {value};"));
            } else {
                self.statements.push(format!("{name} = {value};"));
            }
        } else {
            self.stack_underflow(instruction, 1);
        }
    }

    fn emit_store_slot_from_operand(&mut self, instruction: &Instruction, kind: SlotKind) {
        let Some(index) = Self::slot_index_from_operand(instruction) else {
            self.note(
                instruction,
                &format!("{} missing operand", instruction.opcode.mnemonic()),
            );
            return;
        };
        self.emit_store_slot(instruction, kind, index);
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
        let false_target = target as usize;
        let loop_jump = self.detect_loop_back(false_target, instruction.offset);
        if let Some(loop_jump) = loop_jump.as_ref() {
            self.statements.push(format!("while {condition} {{"));
            self.skip_jumps.insert(loop_jump.jump_offset);
            self.loop_stack.push(LoopContext {
                break_offset: false_target,
                continue_offset: loop_jump.target,
            });
        } else {
            self.statements.push(format!("if {condition} {{"));
        }
        let closer_entry = self.pending_closers.entry(false_target).or_insert(0);
        *closer_entry += 1;

        if loop_jump.is_none() {
            if let Some((jump_offset, jump_target)) = self.detect_else(false_target) {
                if !self.is_loop_control_target(jump_target) {
                    self.skip_jumps.insert(jump_offset);
                    let else_entry = self.else_targets.entry(false_target).or_insert(0);
                    *else_entry += 1;
                    let closer = self.pending_closers.entry(jump_target).or_insert(0);
                    *closer += 1;
                }
            }
        }
    }

    fn emit_relative(&mut self, instruction: &Instruction, width: isize, label: &str) {
        if let Some(target) = self.jump_target(instruction, width) {
            self.note(
                instruction,
                &format!("{label} -> 0x{target:04X} (control flow not yet lifted)"),
            );
        } else {
            self.note(
                instruction,
                &format!("{label} with unsupported operand (skipping)"),
            );
        }
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
        match self.jump_target(instruction, width) {
            Some(target) => {
                if self.try_emit_loop_jump(instruction, target) {
                    return;
                }
                self.note(
                    instruction,
                    &format!("jump -> 0x{target:04X} (control flow not yet lifted)"),
                );
            }
            None => self.note(
                instruction,
                "jump with unsupported operand (skipping)",
            ),
        }
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

    fn slot_label(&self, kind: SlotKind, index: usize) -> String {
        match kind {
            SlotKind::Local => format!("loc{index}"),
            SlotKind::Argument => self
                .argument_labels
                .get(&index)
                .cloned()
                .unwrap_or_else(|| format!("arg{index}")),
            SlotKind::Static => format!("static{index}"),
        }
    }

    fn slot_index_from_operand(instruction: &Instruction) -> Option<usize> {
        match instruction.operand {
            Some(Operand::U8(value)) => Some(value as usize),
            _ => None,
        }
    }

    fn analyze_do_while_loops(&mut self) {
        for instruction in &self.program {
            if !matches!(instruction.opcode, OpCode::Jmpif | OpCode::Jmpif_L) {
                continue;
            }
            let width = Self::branch_width(instruction.opcode);
            if let Some(target) = self.forward_jump_target(instruction, width) {
                if target < instruction.offset {
                    let break_offset = instruction.offset as isize + width;
                    if break_offset >= 0 {
                        self.do_while_headers
                            .entry(target)
                            .or_default()
                            .push(DoWhileLoop {
                                tail_offset: instruction.offset,
                                break_offset: break_offset as usize,
                            });
                    }
                }
            }
        }
    }

    fn try_emit_do_while_tail(&mut self, instruction: &Instruction) -> bool {
        if !self.active_do_while_tails.remove(&instruction.offset) {
            return false;
        }
        let Some(condition) = self.stack.pop() else {
            self.push_comment(instruction);
            self.stack_underflow(instruction, 1);
            return true;
        };
        self.push_comment(instruction);
        self.statements.push(format!("}} while ({condition});"));
        self.pop_loops_with_continue(instruction.offset);
        true
    }

    fn detect_loop_back(
        &self,
        false_offset: usize,
        condition_offset: usize,
    ) -> Option<LoopJump> {
        let (_, &index) = self.index_by_offset.range(..false_offset).next_back()?;
        let jump_instruction = self.program.get(index)?;
        match jump_instruction.opcode {
            OpCode::Jmp | OpCode::Jmp_L => {
                let width = Self::branch_width(jump_instruction.opcode);
                let Some(target) =
                    self.forward_jump_target(jump_instruction, width)
                else {
                    return None;
                };
                if target <= condition_offset
                    && !self
                        .loop_stack
                        .iter()
                        .any(|ctx| ctx.continue_offset == target)
                {
                    return Some(LoopJump {
                        jump_offset: jump_instruction.offset,
                        target,
                    });
                }
            }
            _ => {}
        }
        None
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

    fn jump_target(&self, instruction: &Instruction, width: isize) -> Option<usize> {
        let delta = match instruction.operand {
            Some(Operand::Jump(value)) => value as isize,
            Some(Operand::Jump32(value)) => value as isize,
            _ => return None,
        };
        let target = instruction.offset as isize + width + delta;
        if target < 0 {
            return None;
        }
        Some(target as usize)
    }

    fn try_emit_loop_jump(&mut self, instruction: &Instruction, target: usize) -> bool {
        if self
            .loop_stack
            .iter()
            .rev()
            .any(|ctx| ctx.break_offset == target)
        {
            self.push_comment(instruction);
            self.statements.push("break;".into());
            self.stack.clear();
            return true;
        }
        if self
            .loop_stack
            .iter()
            .rev()
            .any(|ctx| ctx.continue_offset == target)
        {
            self.push_comment(instruction);
            self.statements.push("continue;".into());
            self.stack.clear();
            return true;
        }
        false
    }

    fn close_loops_at(&mut self, offset: usize) {
        while self
            .loop_stack
            .last()
            .map(|ctx| ctx.break_offset == offset)
            .unwrap_or(false)
        {
            self.loop_stack.pop();
        }
    }

    fn pop_loops_with_continue(&mut self, continue_offset: usize) {
        while self
            .loop_stack
            .last()
            .map(|ctx| ctx.continue_offset == continue_offset)
            .unwrap_or(false)
        {
            self.loop_stack.pop();
        }
    }

    fn is_loop_control_target(&self, target: usize) -> bool {
        self
            .loop_stack
            .iter()
            .any(|ctx| ctx.break_offset == target || ctx.continue_offset == target)
    }

    fn rewrite_for_loops(statements: &mut Vec<String>) {
        let mut index = 0;
        while index < statements.len() {
            let Some(condition) = Self::extract_while_condition(&statements[index]) else {
                index += 1;
                continue;
            };
            let Some(end) = Self::find_block_end(statements, index) else {
                index += 1;
                continue;
            };
            let Some(init_idx) = Self::find_initializer_index(statements, index) else {
                index += 1;
                continue;
            };
            let Some(init_assignment) = Self::parse_assignment(&statements[init_idx]) else {
                index += 1;
                continue;
            };
            let Some((increment_idx, temp_idx, increment_expr)) = Self::find_increment_assignment(
                statements,
                index,
                end,
                &init_assignment.lhs,
            ) else {
                index += 1;
                continue;
            };

            statements[index] = format!(
                "for ({}; {}; {}) {{",
                init_assignment.full, condition, increment_expr
            );
            statements[init_idx].clear();
            statements[increment_idx].clear();
            if let Some(temp_idx) = temp_idx {
                statements[temp_idx].clear();
            }
            index += 1;
        }
    }

    fn extract_while_condition(line: &str) -> Option<String> {
        let trimmed = line.trim();
        if !trimmed.starts_with("while ") {
            return None;
        }
        let rest = trimmed.strip_prefix("while ")?;
        let condition = rest.strip_suffix(" {")?.trim();
        if condition.is_empty() {
            None
        } else {
            Some(condition.to_string())
        }
    }

    fn find_block_end(statements: &[String], start: usize) -> Option<usize> {
        let mut depth = Self::brace_delta(&statements[start]);
        let mut index = start + 1;
        while index < statements.len() {
            depth += Self::brace_delta(&statements[index]);
            if depth == 0 {
                return Some(index);
            }
            index += 1;
        }
        None
    }

    fn brace_delta(line: &str) -> isize {
        let openings = line.matches('{').count() as isize;
        let closings = line.matches('}').count() as isize;
        openings - closings
    }

    fn find_initializer_index(statements: &[String], start: usize) -> Option<usize> {
        let mut index = start;
        while index > 0 {
            index -= 1;
            let line = statements[index].trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            if line == "}" || line.ends_with("{") {
                break;
            }
            if line.contains('=') && line.ends_with(';') {
                if let Some(assign) = Self::parse_assignment(line) {
                    if assign.lhs.starts_with("loc")
                        || assign.lhs.starts_with("arg")
                        || assign.lhs.starts_with("static")
                    {
                        return Some(index);
                    }
                }
            }
        }
        None
    }

    fn find_increment_assignment(
        statements: &[String],
        start: usize,
        end: usize,
        var: &str,
    ) -> Option<(usize, Option<usize>, String)> {
        let mut index = end;
        while index > start {
            index -= 1;
            let line = statements[index].trim();
            if line.is_empty() || line.starts_with("//") || line == "}" {
                continue;
            }
            let Some(assign) = Self::parse_assignment(line) else {
                return None;
            };
            if assign.lhs != var {
                return None;
            }
            if assign.rhs.starts_with(var) {
                return Some((index, None, assign.full));
            }
            if let Some(prev_idx) = Self::previous_code_line(statements, index) {
                let Some(prev_assign) = Self::parse_assignment(&statements[prev_idx]) else {
                    return None;
                };
                if prev_assign.lhs == assign.rhs {
                    let expr = format!("{} = {}", var, prev_assign.rhs);
                    return Some((index, Some(prev_idx), expr));
                }
            }
            return None;
        }
        None
    }

    fn previous_code_line(statements: &[String], mut index: usize) -> Option<usize> {
        while index > 0 {
            index -= 1;
            let line = statements[index].trim();
            if line.is_empty() || line.starts_with("//") || line == "}" {
                continue;
            }
            return Some(index);
        }
        None
    }

    fn parse_assignment(line: &str) -> Option<Assignment> {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.ends_with(';') {
            return None;
        }
        let body = trimmed.trim_end_matches(';').trim();
        let mut parts = body.splitn(2, '=');
        let lhs_raw = parts.next()?.trim();
        let rhs = parts.next()?.trim().to_string();
        if lhs_raw.is_empty() || rhs.is_empty() {
            return None;
        }
        let lhs = if let Some(stripped) = lhs_raw.strip_prefix("let ") {
            stripped.trim().to_string()
        } else {
            lhs_raw.to_string()
        };
        Some(Assignment {
            full: body.to_string(),
            lhs,
            rhs,
        })
    }
}

#[derive(Debug, Clone)]
struct Assignment {
    full: String,
    lhs: String,
    rhs: String,
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
        assert!(decompilation.high_level.contains("fn main() -> int {"));
    }

    #[test]
    fn renames_script_entry_using_manifest_signature() {
        let nef_bytes = sample_nef();
        let manifest = ContractManifest::from_json_str(
            r#"
            {
                "name": "Parametrized",
                "abi": {
                    "methods": [
                        {
                            "name": "deploy",
                            "parameters": [
                                {"name": "owner", "type": "Hash160"},
                                {"name": "amount", "type": "Integer"}
                            ],
                            "returntype": "Void",
                            "offset": 0
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*"
            }
            "#,
        )
        .expect("manifest parsed");
        let decompilation = Decompiler::new()
            .decompile_bytes_with_manifest(&nef_bytes, Some(manifest))
            .expect("decompile succeeds with manifest signature");

        assert!(decompilation
            .high_level
            .contains("fn deploy(owner: hash160, amount: int) {"));
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

    #[test]
    fn high_level_lifts_simple_while_loop() {
        // Script: PUSH1, JMPIFNOT +3 (to RET), NOP, JMP -6 (to PUSH1), RET
        let script = [0x11, 0x26, 0x03, 0x21, 0x22, 0xFA, 0x40];
        let nef_bytes = build_nef(&script);
        let decompilation = Decompiler::new()
            .decompile_bytes(&nef_bytes)
            .expect("decompile succeeds");

        let high_level = &decompilation.high_level;
        assert!(high_level.contains("while t0 {"), "missing while block: {high_level}");
        assert!(
            !high_level.contains("jump ->"),
            "loop back-edge should be lifted: {high_level}"
        );
    }

    #[test]
    fn high_level_lifts_do_while_loop() {
        // Script: body; PUSH1; JMPIF -5; RET
        let script = [0x11, 0x21, 0x11, 0x24, 0xFB, 0x40];
        let nef_bytes = build_nef(&script);
        let decompilation = Decompiler::new()
            .decompile_bytes(&nef_bytes)
            .expect("decompile succeeds");

        let high_level = &decompilation.high_level;
        assert!(high_level.contains("do {"), "missing do/while header: {high_level}");
        assert!(
            high_level.contains("} while ("),
            "missing do/while tail: {high_level}"
        );
    }

    #[test]
    fn high_level_lifts_local_slots() {
        // Script: INITSLOT 1,0; PUSH1; STLOC0; LDLOC0; RET
        let script = [0x57, 0x01, 0x00, 0x11, 0x70, 0x68, 0x40];
        let nef_bytes = build_nef(&script);
        let decompilation = Decompiler::new()
            .decompile_bytes(&nef_bytes)
            .expect("decompile succeeds");

        let high_level = &decompilation.high_level;
        assert!(high_level.contains("// declare 1 locals, 0 arguments"));
        assert!(high_level.contains("let loc0 = t0;"));
        assert!(high_level.contains("return loc0;"));
    }

    #[test]
    fn high_level_lifts_for_loop() {
        // Script models: for (loc0 = 0; loc0 < 3; loc0++) {}
        let script = [
            0x57, 0x01, 0x00, 0x10, 0x70, 0x68, 0x13, 0xB5, 0x26, 0x07, 0x21, 0x68, 0x11, 0x9E,
            0x70, 0x22, 0xF4, 0x40,
        ];
        let nef_bytes = build_nef(&script);
        let decompilation = Decompiler::new()
            .decompile_bytes(&nef_bytes)
            .expect("decompile succeeds");

        let high_level = &decompilation.high_level;
        assert!(high_level.contains("for (let loc0 = t0;"), "missing for-loop header: {high_level}");
        assert!(high_level.contains("loc0 = loc0 +"), "increment not surfaced: {high_level}");
    }

    #[test]
    fn high_level_emits_break_and_continue() {
        // Script demonstrating break/continue inside a while loop
        let script = [
            0x57, 0x01, 0x00, 0x10, 0x70, 0x68, 0x13, 0xB5, 0x26, 0x18, 0x68, 0x11, 0xB3, 0x26,
            0x06, 0x68, 0x11, 0x9E, 0x70, 0x22, 0xF0, 0x68, 0x12, 0xB3, 0x26, 0x02, 0x22, 0x06,
            0x68, 0x11, 0x9E, 0x70, 0x22, 0xE3, 0x40,
        ];
        let nef_bytes = build_nef(&script);
        let decompilation = Decompiler::new()
            .decompile_bytes(&nef_bytes)
            .expect("decompile succeeds");

        let high_level = &decompilation.high_level;
        assert!(high_level.contains("break;"), "missing break statement: {high_level}");
        assert!(high_level.contains("continue;"), "missing continue statement: {high_level}");
    }

    #[test]
    fn csharp_view_respects_manifest_metadata_and_parameters() {
        let nef_bytes = sample_nef();
        let manifest = ContractManifest::from_json_str(
            r#"
            {
                "name": "Demo",
                "abi": {
                    "methods": [
                        {
                            "name": "deploy-contract",
                            "parameters": [
                                {"name": "owner-name", "type": "Hash160"},
                                {"name": "amount", "type": "Integer"}
                            ],
                            "returntype": "Void",
                            "offset": 0
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*",
                "extra": {"Author": "Jane Doe", "Email": "jane@example.com"}
            }
            "#,
        )
        .expect("manifest parsed");

        let decompilation = Decompiler::new()
            .decompile_bytes_with_manifest(&nef_bytes, Some(manifest))
            .expect("decompile succeeds");

        let csharp = &decompilation.csharp;
        assert!(csharp.contains("[ManifestExtra(\"Author\", \"Jane Doe\")]"));
        assert!(csharp.contains("[ManifestExtra(\"Email\", \"jane@example.com\")]"));
        assert!(csharp.contains(
            "public static void deploy_contract(UInt160 owner_name, BigInteger amount)"
        ));
    }
}

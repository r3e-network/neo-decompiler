use std::collections::HashSet;

use serde::Serialize;
use serde_json::Value;

use crate::decompiler::analysis::{call_graph::CallGraph, types::TypeInfo, xrefs::Xrefs};
use crate::decompiler::Decompilation;
use crate::disassembler::DisassemblyOutput;
use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::{
    ContractManifest, ManifestPermissionContract, ManifestPermissionMethods, ManifestTrusts,
};
use crate::native_contracts;
use crate::nef::{call_flag_labels, MethodToken, NefFile};
use crate::util;

/// Browser-friendly summary of `info` output.
#[derive(Debug, Clone, Serialize)]
pub struct WebInfoReport {
    compiler: String,
    source: Option<String>,
    script_length: usize,
    script_hash_le: String,
    script_hash_be: String,
    checksum: String,
    method_tokens: Vec<MethodTokenReport>,
    manifest: Option<ManifestSummary>,
    warnings: Vec<String>,
}

/// Browser-friendly summary of `disasm` output.
#[derive(Debug, Clone, Serialize)]
pub struct WebDisasmReport {
    instructions: Vec<InstructionReport>,
    warnings: Vec<String>,
}

/// Browser-friendly summary of `decompile` output.
#[derive(Debug, Clone, Serialize)]
pub struct WebDecompileReport {
    script_hash_le: String,
    script_hash_be: String,
    csharp: String,
    high_level: String,
    pseudocode: String,
    instructions: Vec<InstructionReport>,
    method_tokens: Vec<MethodTokenReport>,
    manifest: Option<ManifestSummary>,
    analysis: AnalysisReport,
    warnings: Vec<String>,
}

pub(super) fn build_info_report(
    nef: &NefFile,
    manifest: Option<&ContractManifest>,
) -> WebInfoReport {
    let script_hash = nef.script_hash();
    let method_tokens: Vec<MethodTokenReport> = nef
        .method_tokens
        .iter()
        .map(build_method_token_report)
        .collect();
    WebInfoReport {
        compiler: nef.header.compiler.trim_end_matches('\0').to_string(),
        source: (!nef.header.source.is_empty()).then(|| nef.header.source.clone()),
        script_length: nef.script.len(),
        script_hash_le: util::format_hash(&script_hash),
        script_hash_be: util::format_hash_be(&script_hash),
        checksum: format!("0x{:08X}", nef.checksum),
        warnings: collect_warnings(&method_tokens),
        method_tokens,
        manifest: manifest.map(summarize_manifest),
    }
}

pub(super) fn build_disasm_report(output: DisassemblyOutput) -> WebDisasmReport {
    WebDisasmReport {
        instructions: output
            .instructions
            .iter()
            .map(InstructionReport::from)
            .collect(),
        warnings: output.warnings.iter().map(ToString::to_string).collect(),
    }
}

pub(super) fn build_decompile_report(result: Decompilation) -> WebDecompileReport {
    let Decompilation {
        nef,
        manifest,
        warnings: decompile_warnings,
        instructions,
        call_graph,
        xrefs,
        types,
        pseudocode,
        high_level,
        csharp,
        ..
    } = result;
    let script_hash = nef.script_hash();
    let method_tokens: Vec<MethodTokenReport> = nef
        .method_tokens
        .iter()
        .map(build_method_token_report)
        .collect();
    let mut warnings = Vec::new();
    let mut seen = HashSet::new();
    for warning in collect_warnings(&method_tokens)
        .into_iter()
        .chain(decompile_warnings.into_iter())
    {
        if seen.insert(warning.clone()) {
            warnings.push(warning);
        }
    }

    WebDecompileReport {
        script_hash_le: util::format_hash(&script_hash),
        script_hash_be: util::format_hash_be(&script_hash),
        csharp: csharp.unwrap_or_default(),
        high_level: high_level.unwrap_or_default(),
        pseudocode: pseudocode.unwrap_or_default(),
        instructions: instructions.iter().map(InstructionReport::from).collect(),
        method_tokens,
        manifest: manifest.as_ref().map(summarize_manifest),
        analysis: AnalysisReport {
            call_graph,
            xrefs,
            types,
        },
        warnings,
    }
}

#[derive(Debug, Clone, Serialize)]
struct AnalysisReport {
    call_graph: CallGraph,
    xrefs: Xrefs,
    types: TypeInfo,
}

#[derive(Debug, Clone, Serialize)]
struct InstructionReport {
    offset: usize,
    opcode: String,
    operand: Option<String>,
    operand_kind: Option<String>,
    operand_value: Option<OperandValueReport>,
    returns_value: Option<bool>,
}

impl From<&Instruction> for InstructionReport {
    fn from(instruction: &Instruction) -> Self {
        InstructionReport {
            offset: instruction.offset,
            opcode: instruction.opcode.mnemonic().to_string(),
            operand: instruction.operand.as_ref().map(ToString::to_string),
            operand_kind: instruction
                .operand
                .as_ref()
                .map(|op| operand_kind_name(op).to_string()),
            operand_value: instruction.operand.as_ref().map(operand_value_report),
            returns_value: returns_value_for_instruction(instruction),
        }
    }
}

fn operand_kind_name(operand: &Operand) -> &'static str {
    match operand {
        Operand::I8(_) => "I8",
        Operand::I16(_) => "I16",
        Operand::I32(_) => "I32",
        Operand::I64(_) => "I64",
        Operand::Bytes(_) => "Bytes",
        Operand::Jump(_) => "Jump8",
        Operand::Jump32(_) => "Jump32",
        Operand::Syscall(_) => "Syscall",
        Operand::U8(_) => "U8",
        Operand::U16(_) => "U16",
        Operand::U32(_) => "U32",
        Operand::Bool(_) => "Bool",
        Operand::Null => "Null",
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "value")]
enum OperandValueReport {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    Bool(bool),
    Bytes(String),
    Jump(i32),
    Jump32(i32),
    Syscall(u32),
    Null,
}

fn operand_value_report(operand: &Operand) -> OperandValueReport {
    match operand {
        Operand::I8(value) => OperandValueReport::I8(*value),
        Operand::I16(value) => OperandValueReport::I16(*value),
        Operand::I32(value) => OperandValueReport::I32(*value),
        Operand::I64(value) => OperandValueReport::I64(*value),
        Operand::U8(value) => OperandValueReport::U8(*value),
        Operand::U16(value) => OperandValueReport::U16(*value),
        Operand::U32(value) => OperandValueReport::U32(*value),
        Operand::Bool(value) => OperandValueReport::Bool(*value),
        Operand::Jump(value) => OperandValueReport::Jump(*value as i32),
        Operand::Jump32(value) => OperandValueReport::Jump32(*value),
        Operand::Syscall(value) => OperandValueReport::Syscall(*value),
        Operand::Bytes(bytes) => {
            OperandValueReport::Bytes(format!("0x{}", util::upper_hex_string(bytes)))
        }
        Operand::Null => OperandValueReport::Null,
    }
}

fn returns_value_for_instruction(instruction: &Instruction) -> Option<bool> {
    if let OpCode::Syscall = instruction.opcode {
        if let Some(Operand::Syscall(hash)) = instruction.operand {
            return crate::syscalls::lookup(hash).map(|info| info.returns_value);
        }
    }
    None
}

#[derive(Debug, Clone, Serialize)]
struct MethodTokenReport {
    method: String,
    hash_le: String,
    hash_be: String,
    parameters: u16,
    returns: bool,
    call_flags: u8,
    call_flag_labels: Vec<&'static str>,
    returns_value: bool,
    native_contract: Option<NativeContractReport>,
    warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct NativeContractReport {
    contract: String,
    method: Option<String>,
    label: String,
}

fn build_method_token_report(token: &MethodToken) -> MethodTokenReport {
    let hint = native_contracts::describe_method_token(&token.hash, &token.method);
    let warning = hint.as_ref().and_then(|h| {
        if h.has_exact_method() {
            None
        } else {
            Some(format!(
                "native contract {} does not expose method {}",
                h.contract, token.method
            ))
        }
    });
    let native_contract = hint.as_ref().map(|h| NativeContractReport {
        contract: h.contract.to_string(),
        method: h.canonical_method.map(ToString::to_string),
        label: h.formatted_label(&token.method),
    });

    MethodTokenReport {
        method: token.method.clone(),
        hash_le: util::format_hash(&token.hash),
        hash_be: util::format_hash_be(&token.hash),
        parameters: token.parameters_count,
        returns: token.has_return_value,
        call_flags: token.call_flags,
        call_flag_labels: call_flag_labels(token.call_flags),
        returns_value: token.has_return_value,
        native_contract,
        warning,
    }
}

fn collect_warnings(tokens: &[MethodTokenReport]) -> Vec<String> {
    tokens
        .iter()
        .filter_map(|report| report.warning.as_ref().map(ToString::to_string))
        .collect()
}

#[derive(Debug, Clone, Serialize)]
struct ManifestSummary {
    name: String,
    supported_standards: Vec<String>,
    storage: bool,
    payable: bool,
    groups: Vec<GroupSummary>,
    methods: usize,
    events: usize,
    permissions: Vec<PermissionSummary>,
    trusts: Option<TrustSummary>,
    abi: AbiSummary,
}

#[derive(Debug, Clone, Serialize)]
struct GroupSummary {
    pubkey: String,
    signature: String,
}

#[derive(Debug, Clone, Serialize)]
struct PermissionSummary {
    contract: PermissionContractSummary,
    methods: PermissionMethodsSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "value")]
enum PermissionContractSummary {
    Wildcard(String),
    Hash(String),
    Group(String),
    Other(Value),
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "value")]
enum PermissionMethodsSummary {
    Wildcard(String),
    Methods(Vec<String>),
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "value")]
enum TrustSummary {
    Wildcard(String),
    Contracts(Vec<String>),
    Other(Value),
}

#[derive(Debug, Clone, Serialize)]
struct AbiSummary {
    methods: Vec<MethodSummary>,
    events: Vec<EventSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct MethodSummary {
    name: String,
    parameters: Vec<ParameterSummary>,
    return_type: String,
    safe: bool,
    offset: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
struct EventSummary {
    name: String,
    parameters: Vec<ParameterSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct ParameterSummary {
    name: String,
    ty: String,
}

fn summarize_manifest(manifest: &ContractManifest) -> ManifestSummary {
    ManifestSummary {
        name: manifest.name.clone(),
        supported_standards: manifest.supported_standards.clone(),
        storage: manifest.features.storage,
        payable: manifest.features.payable,
        groups: manifest
            .groups
            .iter()
            .map(|group| GroupSummary {
                pubkey: group.pubkey.clone(),
                signature: group.signature.clone(),
            })
            .collect(),
        methods: manifest.abi.methods.len(),
        events: manifest.abi.events.len(),
        permissions: manifest
            .permissions
            .iter()
            .map(|permission| PermissionSummary {
                contract: PermissionContractSummary::from(&permission.contract),
                methods: PermissionMethodsSummary::from(&permission.methods),
            })
            .collect(),
        trusts: manifest.trusts.as_ref().map(TrustSummary::from),
        abi: AbiSummary {
            methods: manifest
                .abi
                .methods
                .iter()
                .map(|method| MethodSummary {
                    name: method.name.clone(),
                    parameters: method
                        .parameters
                        .iter()
                        .map(|param| ParameterSummary {
                            name: param.name.clone(),
                            ty: param.kind.clone(),
                        })
                        .collect(),
                    return_type: method.return_type.clone(),
                    safe: method.safe,
                    offset: method.offset,
                })
                .collect(),
            events: manifest
                .abi
                .events
                .iter()
                .map(|event| EventSummary {
                    name: event.name.clone(),
                    parameters: event
                        .parameters
                        .iter()
                        .map(|param| ParameterSummary {
                            name: param.name.clone(),
                            ty: param.kind.clone(),
                        })
                        .collect(),
                })
                .collect(),
        },
    }
}

impl From<&ManifestPermissionContract> for PermissionContractSummary {
    fn from(contract: &ManifestPermissionContract) -> Self {
        match contract {
            ManifestPermissionContract::Wildcard(value) => {
                PermissionContractSummary::Wildcard(value.clone())
            }
            ManifestPermissionContract::Hash { hash } => {
                PermissionContractSummary::Hash(hash.clone())
            }
            ManifestPermissionContract::Group { group } => {
                PermissionContractSummary::Group(group.clone())
            }
            ManifestPermissionContract::Other(value) => {
                PermissionContractSummary::Other(value.clone())
            }
        }
    }
}

impl From<&ManifestPermissionMethods> for PermissionMethodsSummary {
    fn from(methods: &ManifestPermissionMethods) -> Self {
        match methods {
            ManifestPermissionMethods::Wildcard(value) => {
                PermissionMethodsSummary::Wildcard(value.clone())
            }
            ManifestPermissionMethods::Methods(list) => {
                PermissionMethodsSummary::Methods(list.clone())
            }
        }
    }
}

impl From<&ManifestTrusts> for TrustSummary {
    fn from(trusts: &ManifestTrusts) -> Self {
        match trusts {
            ManifestTrusts::Wildcard(value) => TrustSummary::Wildcard(value.clone()),
            ManifestTrusts::Contracts(values) => TrustSummary::Contracts(values.clone()),
            ManifestTrusts::Other(value) => TrustSummary::Other(value.clone()),
        }
    }
}

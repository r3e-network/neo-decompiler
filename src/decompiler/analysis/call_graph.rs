//! Call graph construction for Neo N3 scripts.

// Bytecode offset arithmetic requires isize↔usize casts for signed jump deltas.
// NEF scripts are bounded (~1 MB), so these conversions are structurally safe.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use std::collections::BTreeMap;

use serde::Serialize;

use crate::instruction::{Instruction, OpCode, Operand, OperandEncoding};
use crate::manifest::ContractManifest;
use crate::nef::NefFile;
use crate::{syscalls, util};

use super::{MethodRef, MethodTable};

/// A resolved call target extracted from the instruction stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum CallTarget {
    /// Direct call into the same script (CALL/CALL_L).
    Internal {
        /// Callee method resolved from the target offset.
        method: MethodRef,
    },
    /// Call to an entry in the NEF method-token table (CALLT).
    MethodToken {
        /// Index into the NEF `method_tokens` table.
        index: u16,
        /// Script hash (little-endian) for the called contract.
        hash_le: String,
        /// Script hash (big-endian) for the called contract.
        hash_be: String,
        /// Target method name.
        method: String,
        /// Declared parameter count.
        parameters_count: u16,
        /// Whether the target method has a return value.
        has_return_value: bool,
        /// Call flags bitfield.
        call_flags: u8,
    },
    /// System call (SYSCALL).
    Syscall {
        /// Syscall identifier (little-endian u32).
        hash: u32,
        /// Resolved syscall name when known.
        name: Option<String>,
        /// Whether the syscall is known to push a value.
        returns_value: bool,
    },
    /// Indirect call (e.g., CALLA) where the destination cannot be resolved statically.
    Indirect {
        /// Opcode mnemonic (`CALLA` or similar).
        opcode: String,
        /// Optional operand value (when present).
        operand: Option<u16>,
    },
    /// A CALL/CALL_L target that could not be resolved to a valid offset.
    UnresolvedInternal {
        /// Computed target offset (may be negative when malformed).
        target: isize,
    },
}

/// One call edge in the call graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallEdge {
    /// Caller method containing the call instruction.
    pub caller: MethodRef,
    /// Bytecode offset of the call instruction.
    pub call_offset: usize,
    /// Opcode mnemonic of the call instruction (e.g., `CALL_L`, `SYSCALL`).
    pub opcode: String,
    /// Resolved target.
    pub target: CallTarget,
}

/// Call graph for a decompiled script.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CallGraph {
    /// Known methods (manifest-defined plus synthetic internal targets).
    pub methods: Vec<MethodRef>,
    /// Call edges extracted from the instruction stream.
    pub edges: Vec<CallEdge>,
}

/// Build a call graph for the provided instruction stream.
#[must_use]
pub fn build_call_graph(
    nef: &NefFile,
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
) -> CallGraph {
    let table = MethodTable::new(instructions, manifest);
    let mut methods: BTreeMap<usize, MethodRef> = table
        .spans()
        .iter()
        .map(|span| (span.method.offset, span.method.clone()))
        .collect();

    let mut edges = Vec::new();
    for (index, instr) in instructions.iter().enumerate() {
        match instr.opcode {
            OpCode::Syscall => {
                let Some(Operand::Syscall(hash)) = instr.operand else {
                    continue;
                };
                let info = syscalls::lookup(hash);
                edges.push(CallEdge {
                    caller: table.method_for_offset(instr.offset),
                    call_offset: instr.offset,
                    opcode: instr.opcode.to_string(),
                    target: CallTarget::Syscall {
                        hash,
                        name: info.map(|i| i.name.to_string()),
                        returns_value: info.map(|i| i.returns_value).unwrap_or(true),
                    },
                });
            }
            OpCode::Call | OpCode::Call_L => {
                let caller = table.method_for_offset(instr.offset);
                match relative_target_isize(instructions, index, instr) {
                    Some(target) if target >= 0 => {
                        let target = target as usize;
                        let callee = table.resolve_internal_target(target);
                        methods.insert(callee.offset, callee.clone());
                        edges.push(CallEdge {
                            caller,
                            call_offset: instr.offset,
                            opcode: instr.opcode.to_string(),
                            target: CallTarget::Internal { method: callee },
                        });
                    }
                    Some(target) => edges.push(CallEdge {
                        caller,
                        call_offset: instr.offset,
                        opcode: instr.opcode.to_string(),
                        target: CallTarget::UnresolvedInternal { target },
                    }),
                    None => edges.push(CallEdge {
                        caller,
                        call_offset: instr.offset,
                        opcode: instr.opcode.to_string(),
                        target: CallTarget::UnresolvedInternal { target: -1 },
                    }),
                }
            }
            OpCode::CallT => {
                let Some(Operand::U16(index)) = instr.operand else {
                    continue;
                };
                let token = nef.method_tokens.get(index as usize);
                if let Some(token) = token {
                    edges.push(CallEdge {
                        caller: table.method_for_offset(instr.offset),
                        call_offset: instr.offset,
                        opcode: instr.opcode.to_string(),
                        target: CallTarget::MethodToken {
                            index,
                            hash_le: util::format_hash(&token.hash),
                            hash_be: util::format_hash_be(&token.hash),
                            method: token.method.clone(),
                            parameters_count: token.parameters_count,
                            has_return_value: token.has_return_value,
                            call_flags: token.call_flags,
                        },
                    });
                } else {
                    edges.push(CallEdge {
                        caller: table.method_for_offset(instr.offset),
                        call_offset: instr.offset,
                        opcode: instr.opcode.to_string(),
                        target: CallTarget::Indirect {
                            opcode: instr.opcode.to_string(),
                            operand: Some(index),
                        },
                    });
                }
            }
            OpCode::CallA => {
                // CALLA takes no operand — it pops a Pointer from the stack.
                edges.push(CallEdge {
                    caller: table.method_for_offset(instr.offset),
                    call_offset: instr.offset,
                    opcode: instr.opcode.to_string(),
                    target: CallTarget::Indirect {
                        opcode: instr.opcode.to_string(),
                        operand: None,
                    },
                });
            }
            _ => {}
        }
    }

    CallGraph {
        methods: methods.into_values().collect(),
        edges,
    }
}

fn relative_target_isize(
    instructions: &[Instruction],
    index: usize,
    instr: &Instruction,
) -> Option<isize> {
    let delta = match &instr.operand {
        Some(Operand::Jump(v)) => *v as isize,
        Some(Operand::Jump32(v)) => *v as isize,
        _ => return None,
    };
    let base = instructions
        .get(index + 1)
        .map(|ins| ins.offset)
        .unwrap_or_else(|| instr.offset + instr_len_fallback(instr));
    Some(base as isize + delta)
}

fn instr_len_fallback(instr: &Instruction) -> usize {
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
        OperandEncoding::Data1 => 1 + 1 + bytes_len(instr),
        OperandEncoding::Data2 => 1 + 2 + bytes_len(instr),
        OperandEncoding::Data4 => 1 + 4 + bytes_len(instr),
    }
}

fn bytes_len(instr: &Instruction) -> usize {
    match &instr.operand {
        Some(Operand::Bytes(bytes)) => bytes.len(),
        _ => 0,
    }
}

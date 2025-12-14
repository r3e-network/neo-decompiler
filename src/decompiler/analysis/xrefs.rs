//! Cross-reference analysis for local/argument/static slots.
//!
//! This module records where stack slot values are read and written, keyed by
//! bytecode offset. The result is primarily intended for diagnostics and future
//! data-flow passes.

use serde::Serialize;

use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::ContractManifest;

use super::{MethodRef, MethodTable};

/// Slot kinds addressable by `LD*`/`ST*` opcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[non_exhaustive]
pub enum SlotKind {
    /// Local slot (`LDLOC*` / `STLOC*`).
    #[serde(rename = "local")]
    Local,
    /// Argument slot (`LDARG*` / `STARG*`).
    #[serde(rename = "argument")]
    Argument,
    /// Static slot (`LDSFLD*` / `STSFLD*`).
    #[serde(rename = "static")]
    Static,
}

/// Read/write offsets for one slot index.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SlotXref {
    /// Slot index.
    pub index: usize,
    /// Offsets where the slot value is read.
    pub reads: Vec<usize>,
    /// Offsets where the slot value is written.
    pub writes: Vec<usize>,
}

/// Cross-references for a single method range.
#[derive(Debug, Clone, Serialize)]
pub struct MethodXrefs {
    /// Method containing the reads/writes.
    pub method: MethodRef,
    /// Local-slot references.
    pub locals: Vec<SlotXref>,
    /// Argument-slot references.
    pub arguments: Vec<SlotXref>,
    /// Static-slot references.
    pub statics: Vec<SlotXref>,
}

/// Aggregated cross-reference information across all discovered methods.
#[derive(Debug, Clone, Serialize)]
pub struct Xrefs {
    /// Per-method slot cross-references.
    pub methods: Vec<MethodXrefs>,
}

/// Build cross-reference information for locals/arguments/statics.
#[must_use]
pub fn build_xrefs(instructions: &[Instruction], manifest: Option<&ContractManifest>) -> Xrefs {
    let table = MethodTable::new(instructions, manifest);
    let static_count = scan_static_slot_count(instructions).unwrap_or(0);

    let mut methods = Vec::new();
    for span in table.spans() {
        let slice: Vec<&Instruction> = instructions
            .iter()
            .filter(|ins| ins.offset >= span.start && ins.offset < span.end)
            .collect();
        let (locals_count, args_count) = scan_slot_counts(&slice).unwrap_or((0, 0));

        let mut method_xrefs = MethodXrefs {
            method: span.method.clone(),
            locals: (0..locals_count)
                .map(|index| SlotXref {
                    index,
                    ..SlotXref::default()
                })
                .collect(),
            arguments: (0..args_count)
                .map(|index| SlotXref {
                    index,
                    ..SlotXref::default()
                })
                .collect(),
            statics: (0..static_count)
                .map(|index| SlotXref {
                    index,
                    ..SlotXref::default()
                })
                .collect(),
        };

        for instr in &slice {
            if let Some((kind, index, is_write)) = slot_access(instr) {
                let target = match kind {
                    SlotKind::Local => &mut method_xrefs.locals,
                    SlotKind::Argument => &mut method_xrefs.arguments,
                    SlotKind::Static => &mut method_xrefs.statics,
                };
                if index >= target.len() {
                    let start = target.len();
                    target.extend((start..=index).map(|idx| SlotXref {
                        index: idx,
                        ..SlotXref::default()
                    }));
                }
                let entry = &mut target[index];
                entry.index = index;
                if is_write {
                    entry.writes.push(instr.offset);
                } else {
                    entry.reads.push(instr.offset);
                }
            }
        }

        methods.push(method_xrefs);
    }

    Xrefs { methods }
}

fn slot_access(instr: &Instruction) -> Option<(SlotKind, usize, bool)> {
    use OpCode::*;

    match instr.opcode {
        // locals
        Ldloc0 => Some((SlotKind::Local, 0, false)),
        Ldloc1 => Some((SlotKind::Local, 1, false)),
        Ldloc2 => Some((SlotKind::Local, 2, false)),
        Ldloc3 => Some((SlotKind::Local, 3, false)),
        Ldloc4 => Some((SlotKind::Local, 4, false)),
        Ldloc5 => Some((SlotKind::Local, 5, false)),
        Ldloc6 => Some((SlotKind::Local, 6, false)),
        Ldloc => slot_from_operand(SlotKind::Local, instr.operand.as_ref(), false),
        Stloc0 => Some((SlotKind::Local, 0, true)),
        Stloc1 => Some((SlotKind::Local, 1, true)),
        Stloc2 => Some((SlotKind::Local, 2, true)),
        Stloc3 => Some((SlotKind::Local, 3, true)),
        Stloc4 => Some((SlotKind::Local, 4, true)),
        Stloc5 => Some((SlotKind::Local, 5, true)),
        Stloc6 => Some((SlotKind::Local, 6, true)),
        Stloc => slot_from_operand(SlotKind::Local, instr.operand.as_ref(), true),

        // arguments
        Ldarg0 => Some((SlotKind::Argument, 0, false)),
        Ldarg1 => Some((SlotKind::Argument, 1, false)),
        Ldarg2 => Some((SlotKind::Argument, 2, false)),
        Ldarg3 => Some((SlotKind::Argument, 3, false)),
        Ldarg4 => Some((SlotKind::Argument, 4, false)),
        Ldarg5 => Some((SlotKind::Argument, 5, false)),
        Ldarg6 => Some((SlotKind::Argument, 6, false)),
        Ldarg => slot_from_operand(SlotKind::Argument, instr.operand.as_ref(), false),
        Starg0 => Some((SlotKind::Argument, 0, true)),
        Starg1 => Some((SlotKind::Argument, 1, true)),
        Starg2 => Some((SlotKind::Argument, 2, true)),
        Starg3 => Some((SlotKind::Argument, 3, true)),
        Starg4 => Some((SlotKind::Argument, 4, true)),
        Starg5 => Some((SlotKind::Argument, 5, true)),
        Starg6 => Some((SlotKind::Argument, 6, true)),
        Starg => slot_from_operand(SlotKind::Argument, instr.operand.as_ref(), true),

        // statics
        Ldsfld0 => Some((SlotKind::Static, 0, false)),
        Ldsfld1 => Some((SlotKind::Static, 1, false)),
        Ldsfld2 => Some((SlotKind::Static, 2, false)),
        Ldsfld3 => Some((SlotKind::Static, 3, false)),
        Ldsfld4 => Some((SlotKind::Static, 4, false)),
        Ldsfld5 => Some((SlotKind::Static, 5, false)),
        Ldsfld6 => Some((SlotKind::Static, 6, false)),
        Ldsfld => slot_from_operand(SlotKind::Static, instr.operand.as_ref(), false),
        Stsfld0 => Some((SlotKind::Static, 0, true)),
        Stsfld1 => Some((SlotKind::Static, 1, true)),
        Stsfld2 => Some((SlotKind::Static, 2, true)),
        Stsfld3 => Some((SlotKind::Static, 3, true)),
        Stsfld4 => Some((SlotKind::Static, 4, true)),
        Stsfld5 => Some((SlotKind::Static, 5, true)),
        Stsfld6 => Some((SlotKind::Static, 6, true)),
        Stsfld => slot_from_operand(SlotKind::Static, instr.operand.as_ref(), true),

        _ => None,
    }
}

fn slot_from_operand(
    kind: SlotKind,
    operand: Option<&Operand>,
    is_write: bool,
) -> Option<(SlotKind, usize, bool)> {
    let Some(Operand::U8(index)) = operand else {
        return None;
    };
    Some((kind, *index as usize, is_write))
}

fn scan_slot_counts(instructions: &[&Instruction]) -> Option<(usize, usize)> {
    for instr in instructions {
        if instr.opcode != OpCode::Initslot {
            continue;
        }
        if let Some(Operand::Bytes(bytes)) = &instr.operand {
            if bytes.len() == 2 {
                return Some((bytes[0] as usize, bytes[1] as usize));
            }
        }
    }
    None
}

fn scan_static_slot_count(instructions: &[Instruction]) -> Option<usize> {
    for instr in instructions {
        if instr.opcode != OpCode::Initsslot {
            continue;
        }
        if let Some(Operand::U8(count)) = &instr.operand {
            return Some(*count as usize);
        }
    }
    None
}

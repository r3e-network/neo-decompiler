//! Lightweight type inference for lifted Neo N3 bytecode.
//!
//! The Neo VM is dynamically typed and most syscalls do not encode argument
//! signatures in the bytecode. The goal of this module is therefore to provide
//! a best-effort type recovery pass that is:
//!
//! - conservative (falls back to `unknown`/`any` rather than guessing)
//! - useful for collection recovery and readability improvements
//! - deterministic and panic-free on malformed input

use serde::Serialize;

use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::ContractManifest;

use super::{MethodRef, MethodTable};

/// Primitive/value types inferred from the instruction stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[non_exhaustive]
pub enum ValueType {
    /// Unknown or not yet inferred.
    #[serde(rename = "unknown")]
    Unknown,
    /// Dynamic `any` value.
    #[serde(rename = "any")]
    Any,
    /// Null literal.
    #[serde(rename = "null")]
    Null,
    /// Boolean.
    #[serde(rename = "bool")]
    Boolean,
    /// Integer.
    #[serde(rename = "integer")]
    Integer,
    /// ByteString.
    #[serde(rename = "bytestring")]
    ByteString,
    /// Buffer.
    #[serde(rename = "buffer")]
    Buffer,
    /// Array.
    #[serde(rename = "array")]
    Array,
    /// Struct.
    #[serde(rename = "struct")]
    Struct,
    /// Map.
    #[serde(rename = "map")]
    Map,
    /// Interop interface.
    #[serde(rename = "interopinterface")]
    InteropInterface,
    /// Pointer.
    #[serde(rename = "pointer")]
    Pointer,
}

impl ValueType {
    fn join(self, other: Self) -> Self {
        use ValueType::*;
        if self == other {
            return self;
        }
        match (self, other) {
            (Unknown, x) | (x, Unknown) => x,
            (Null, _) | (_, Null) => Any,
            _ => Any,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct StackValue {
    ty: ValueType,
    int_literal: Option<i64>,
}

impl StackValue {
    fn unknown() -> Self {
        Self {
            ty: ValueType::Unknown,
            int_literal: None,
        }
    }

    fn with_type(ty: ValueType) -> Self {
        Self {
            ty,
            int_literal: None,
        }
    }

    fn integer_literal(value: i64) -> Self {
        Self {
            ty: ValueType::Integer,
            int_literal: Some(value),
        }
    }
}

/// Per-method inferred types.
#[derive(Debug, Clone, Serialize)]
pub struct MethodTypes {
    /// Method whose slots were analyzed.
    pub method: MethodRef,
    /// Inferred argument types indexed by argument slot.
    pub arguments: Vec<ValueType>,
    /// Inferred local types indexed by local slot.
    pub locals: Vec<ValueType>,
}

/// Aggregated type inference results.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TypeInfo {
    /// Per-method inferred locals/arguments.
    pub methods: Vec<MethodTypes>,
    /// Inferred static slot types indexed by static slot.
    pub statics: Vec<ValueType>,
}

/// Infer primitive types and collection kinds from the instruction stream.
#[must_use]
pub fn infer_types(instructions: &[Instruction], manifest: Option<&ContractManifest>) -> TypeInfo {
    let table = MethodTable::new(instructions, manifest);
    let static_count = scan_static_slot_count(instructions).unwrap_or(0);
    let mut statics = vec![ValueType::Unknown; static_count];

    let mut methods = Vec::new();
    for span in table.spans() {
        let slice: Vec<&Instruction> = instructions
            .iter()
            .filter(|ins| ins.offset >= span.start && ins.offset < span.end)
            .collect();

        let (locals_count, args_count) = scan_slot_counts(&slice).unwrap_or((0, 0));
        let mut locals = vec![ValueType::Unknown; locals_count];
        let mut arguments = vec![ValueType::Unknown; args_count];

        if let Some(manifest) = manifest {
            if let Some(index) = table.manifest_index_for_start(span.start) {
                if let Some(method) = manifest.abi.methods.get(index) {
                    for (idx, param) in method.parameters.iter().enumerate() {
                        if idx < arguments.len() {
                            arguments[idx] = arguments[idx].join(type_from_manifest(&param.kind));
                        }
                    }
                }
            }
        }

        infer_types_in_slice(&slice, &mut locals, &mut arguments, &mut statics);

        methods.push(MethodTypes {
            method: span.method.clone(),
            arguments,
            locals,
        });
    }

    TypeInfo { methods, statics }
}

fn infer_types_in_slice(
    instructions: &[&Instruction],
    locals: &mut Vec<ValueType>,
    arguments: &mut Vec<ValueType>,
    statics: &mut Vec<ValueType>,
) {
    let mut stack: Vec<StackValue> = Vec::new();

    for instr in instructions {
        match instr.opcode {
            OpCode::Initsslot => {
                if let Some(Operand::U8(count)) = &instr.operand {
                    let need = *count as usize;
                    if statics.len() < need {
                        statics.resize(need, ValueType::Unknown);
                    }
                }
            }
            OpCode::Initslot => {
                // Operand is 2 bytes: locals, args.
                if let Some(Operand::Bytes(bytes)) = &instr.operand {
                    if bytes.len() == 2 {
                        let locals_count = bytes[0] as usize;
                        let args_count = bytes[1] as usize;
                        if locals.len() < locals_count {
                            locals.resize(locals_count, ValueType::Unknown);
                        }
                        if arguments.len() < args_count {
                            arguments.resize(args_count, ValueType::Unknown);
                        }
                    }
                }
            }

            // Literals
            OpCode::PushNull => stack.push(StackValue::with_type(ValueType::Null)),
            OpCode::PushT | OpCode::PushF => stack.push(StackValue::with_type(ValueType::Boolean)),
            OpCode::Pushdata1 | OpCode::Pushdata2 | OpCode::Pushdata4 => {
                stack.push(StackValue::with_type(ValueType::ByteString));
            }
            OpCode::Pushint8
            | OpCode::Pushint16
            | OpCode::Pushint32
            | OpCode::Pushint64
            | OpCode::Pushint128
            | OpCode::Pushint256
            | OpCode::PushM1
            | OpCode::Push0
            | OpCode::Push1
            | OpCode::Push2
            | OpCode::Push3
            | OpCode::Push4
            | OpCode::Push5
            | OpCode::Push6
            | OpCode::Push7
            | OpCode::Push8
            | OpCode::Push9
            | OpCode::Push10
            | OpCode::Push11
            | OpCode::Push12
            | OpCode::Push13
            | OpCode::Push14
            | OpCode::Push15
            | OpCode::Push16 => {
                if let Some(lit) = int_literal_from_operand(instr.operand.as_ref()) {
                    stack.push(StackValue::integer_literal(lit));
                } else {
                    stack.push(StackValue::with_type(ValueType::Integer));
                }
            }
            OpCode::PushA => stack.push(StackValue::with_type(ValueType::Pointer)),

            // Stack manipulation
            OpCode::Clear => stack.clear(),
            OpCode::Depth => stack.push(StackValue::integer_literal(stack.len() as i64)),
            OpCode::Drop => {
                let _ = pop_or_unknown(&mut stack);
            }
            OpCode::Dup => {
                let value = stack.last().copied().unwrap_or_else(StackValue::unknown);
                stack.push(value);
            }
            OpCode::Swap => {
                if stack.len() >= 2 {
                    let len = stack.len();
                    stack.swap(len - 1, len - 2);
                }
            }
            OpCode::Over => {
                let value = stack
                    .get(stack.len().saturating_sub(2))
                    .copied()
                    .unwrap_or_else(StackValue::unknown);
                stack.push(value);
            }
            OpCode::Nip => {
                if stack.len() >= 2 {
                    let len = stack.len();
                    stack.remove(len - 2);
                }
            }
            OpCode::Rot => {
                if stack.len() >= 3 {
                    let top = stack.pop().unwrap();
                    let mid = stack.pop().unwrap();
                    let bottom = stack.pop().unwrap();
                    stack.push(mid);
                    stack.push(top);
                    stack.push(bottom);
                }
            }
            OpCode::Tuck => {
                if stack.len() >= 2 {
                    let top = stack.pop().unwrap();
                    let second = stack.pop().unwrap();
                    stack.push(top);
                    stack.push(second);
                    stack.push(top);
                }
            }
            OpCode::Pick => {
                let index = pop_or_unknown(&mut stack);
                if let Some(depth) = index.int_literal.and_then(|v| usize::try_from(v).ok()) {
                    let pos = stack.len().checked_sub(1 + depth);
                    if let Some(pos) = pos {
                        if let Some(value) = stack.get(pos).copied() {
                            stack.push(value);
                            continue;
                        }
                    }
                }
                stack.push(StackValue::unknown());
            }
            OpCode::Roll => {
                let index = pop_or_unknown(&mut stack);
                if let Some(depth) = index.int_literal.and_then(|v| usize::try_from(v).ok()) {
                    if depth < stack.len() {
                        let pos = stack.len() - 1 - depth;
                        let value = stack.remove(pos);
                        stack.push(value);
                    }
                }
            }
            OpCode::Xdrop => {
                let index = pop_or_unknown(&mut stack);
                if let Some(depth) = index.int_literal.and_then(|v| usize::try_from(v).ok()) {
                    if depth < stack.len() {
                        let pos = stack.len() - 1 - depth;
                        stack.remove(pos);
                        continue;
                    }
                }
                let _ = pop_or_unknown(&mut stack);
            }
            OpCode::Reverse3 => reverse_top(&mut stack, 3),
            OpCode::Reverse4 => reverse_top(&mut stack, 4),
            OpCode::Reversen => {
                let count = pop_or_unknown(&mut stack);
                if let Some(depth) = count.int_literal.and_then(|v| usize::try_from(v).ok()) {
                    reverse_top(&mut stack, depth);
                }
            }

            // Slot ops
            OpCode::Ldloc0 => push_slot(&mut stack, locals.first().copied()),
            OpCode::Ldloc1 => push_slot(&mut stack, locals.get(1).copied()),
            OpCode::Ldloc2 => push_slot(&mut stack, locals.get(2).copied()),
            OpCode::Ldloc3 => push_slot(&mut stack, locals.get(3).copied()),
            OpCode::Ldloc4 => push_slot(&mut stack, locals.get(4).copied()),
            OpCode::Ldloc5 => push_slot(&mut stack, locals.get(5).copied()),
            OpCode::Ldloc6 => push_slot(&mut stack, locals.get(6).copied()),
            OpCode::Ldloc => push_indexed_slot(&mut stack, locals, instr.operand.as_ref()),
            OpCode::Stloc0 => store_slot(&mut stack, locals, 0),
            OpCode::Stloc1 => store_slot(&mut stack, locals, 1),
            OpCode::Stloc2 => store_slot(&mut stack, locals, 2),
            OpCode::Stloc3 => store_slot(&mut stack, locals, 3),
            OpCode::Stloc4 => store_slot(&mut stack, locals, 4),
            OpCode::Stloc5 => store_slot(&mut stack, locals, 5),
            OpCode::Stloc6 => store_slot(&mut stack, locals, 6),
            OpCode::Stloc => store_indexed_slot(&mut stack, locals, instr.operand.as_ref()),

            OpCode::Ldarg0 => push_slot(&mut stack, arguments.first().copied()),
            OpCode::Ldarg1 => push_slot(&mut stack, arguments.get(1).copied()),
            OpCode::Ldarg2 => push_slot(&mut stack, arguments.get(2).copied()),
            OpCode::Ldarg3 => push_slot(&mut stack, arguments.get(3).copied()),
            OpCode::Ldarg4 => push_slot(&mut stack, arguments.get(4).copied()),
            OpCode::Ldarg5 => push_slot(&mut stack, arguments.get(5).copied()),
            OpCode::Ldarg6 => push_slot(&mut stack, arguments.get(6).copied()),
            OpCode::Ldarg => push_indexed_slot(&mut stack, arguments, instr.operand.as_ref()),
            OpCode::Starg0 => store_slot(&mut stack, arguments, 0),
            OpCode::Starg1 => store_slot(&mut stack, arguments, 1),
            OpCode::Starg2 => store_slot(&mut stack, arguments, 2),
            OpCode::Starg3 => store_slot(&mut stack, arguments, 3),
            OpCode::Starg4 => store_slot(&mut stack, arguments, 4),
            OpCode::Starg5 => store_slot(&mut stack, arguments, 5),
            OpCode::Starg6 => store_slot(&mut stack, arguments, 6),
            OpCode::Starg => store_indexed_slot(&mut stack, arguments, instr.operand.as_ref()),

            OpCode::Ldsfld0 => push_slot(&mut stack, statics.first().copied()),
            OpCode::Ldsfld1 => push_slot(&mut stack, statics.get(1).copied()),
            OpCode::Ldsfld2 => push_slot(&mut stack, statics.get(2).copied()),
            OpCode::Ldsfld3 => push_slot(&mut stack, statics.get(3).copied()),
            OpCode::Ldsfld4 => push_slot(&mut stack, statics.get(4).copied()),
            OpCode::Ldsfld5 => push_slot(&mut stack, statics.get(5).copied()),
            OpCode::Ldsfld6 => push_slot(&mut stack, statics.get(6).copied()),
            OpCode::Ldsfld => push_indexed_slot(&mut stack, statics, instr.operand.as_ref()),
            OpCode::Stsfld0 => store_slot(&mut stack, statics, 0),
            OpCode::Stsfld1 => store_slot(&mut stack, statics, 1),
            OpCode::Stsfld2 => store_slot(&mut stack, statics, 2),
            OpCode::Stsfld3 => store_slot(&mut stack, statics, 3),
            OpCode::Stsfld4 => store_slot(&mut stack, statics, 4),
            OpCode::Stsfld5 => store_slot(&mut stack, statics, 5),
            OpCode::Stsfld6 => store_slot(&mut stack, statics, 6),
            OpCode::Stsfld => store_indexed_slot(&mut stack, statics, instr.operand.as_ref()),

            // Collections
            OpCode::Newarray0 => stack.push(StackValue::with_type(ValueType::Array)),
            OpCode::Newarray => {
                let _ = pop_or_unknown(&mut stack); // count
                stack.push(StackValue::with_type(ValueType::Array));
            }
            OpCode::Newmap => stack.push(StackValue::with_type(ValueType::Map)),
            OpCode::Newstruct0 => stack.push(StackValue::with_type(ValueType::Struct)),
            OpCode::Newstruct => {
                let _ = pop_or_unknown(&mut stack); // count
                stack.push(StackValue::with_type(ValueType::Struct));
            }
            OpCode::Newbuffer => {
                let _ = pop_or_unknown(&mut stack); // length
                stack.push(StackValue::with_type(ValueType::Buffer));
            }
            OpCode::Pack => {
                let count = pop_or_unknown(&mut stack);
                if let Some(count) = count.int_literal.and_then(|v| usize::try_from(v).ok()) {
                    for _ in 0..count {
                        let _ = pop_or_unknown(&mut stack);
                    }
                }
                stack.push(StackValue::with_type(ValueType::Array));
            }
            OpCode::Packmap => {
                let count = pop_or_unknown(&mut stack);
                if let Some(count) = count.int_literal.and_then(|v| usize::try_from(v).ok()) {
                    for _ in 0..count {
                        let _ = pop_or_unknown(&mut stack);
                    }
                }
                stack.push(StackValue::with_type(ValueType::Map));
            }
            OpCode::Packstruct => {
                let count = pop_or_unknown(&mut stack);
                if let Some(count) = count.int_literal.and_then(|v| usize::try_from(v).ok()) {
                    for _ in 0..count {
                        let _ = pop_or_unknown(&mut stack);
                    }
                }
                stack.push(StackValue::with_type(ValueType::Struct));
            }
            OpCode::Unpack => {
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::unknown());
            }
            OpCode::Pickitem => {
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::unknown());
            }
            OpCode::Setitem => {
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
            }
            OpCode::Append => {
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
            }
            OpCode::Remove => {
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
            }
            OpCode::Clearitems => {
                let _ = pop_or_unknown(&mut stack);
            }
            OpCode::Popitem => {
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::unknown());
            }
            OpCode::Size => {
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::with_type(ValueType::Integer));
            }
            OpCode::Haskey => {
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::with_type(ValueType::Boolean));
            }
            OpCode::Isnull => {
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::with_type(ValueType::Boolean));
            }
            OpCode::Istype => {
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::with_type(ValueType::Boolean));
            }
            OpCode::Convert => {
                let value = pop_or_unknown(&mut stack);
                let target = instr
                    .operand
                    .as_ref()
                    .and_then(convert_target_type)
                    .unwrap_or(ValueType::Any);
                stack.push(StackValue::with_type(target.join(value.ty)));
            }

            // Arithmetic + comparisons (subset)
            OpCode::Add
            | OpCode::Sub
            | OpCode::Mul
            | OpCode::Div
            | OpCode::Mod
            | OpCode::Pow
            | OpCode::Min
            | OpCode::Max
            | OpCode::Shl
            | OpCode::Shr => {
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::with_type(ValueType::Integer));
            }
            OpCode::Modmul | OpCode::Modpow => {
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::with_type(ValueType::Integer));
            }
            OpCode::Within => {
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::with_type(ValueType::Boolean));
            }
            OpCode::Sqrt
            | OpCode::Abs
            | OpCode::Sign
            | OpCode::Inc
            | OpCode::Dec
            | OpCode::Negate => {
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::with_type(ValueType::Integer));
            }
            OpCode::And | OpCode::Or | OpCode::Xor => {
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::with_type(ValueType::Integer));
            }
            OpCode::Invert => {
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::with_type(ValueType::Integer));
            }
            OpCode::Not => {
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::with_type(ValueType::Boolean));
            }
            OpCode::Booland | OpCode::Boolor => {
                let _ = pop_or_unknown(&mut stack);
                let _ = pop_or_unknown(&mut stack);
                stack.push(StackValue::with_type(ValueType::Boolean));
            }
            OpCode::Equal
            | OpCode::Numequal
            | OpCode::Notequal
            | OpCode::Numnotequal
            | OpCode::Gt
            | OpCode::Ge
            | OpCode::Lt
            | OpCode::Le
            | OpCode::Nz => {
                // NZ is unary, but treating it as "pop 1" is enough for type recovery.
                let _ = pop_or_unknown(&mut stack);
                if !matches!(instr.opcode, OpCode::Nz) {
                    let _ = pop_or_unknown(&mut stack);
                }
                stack.push(StackValue::with_type(ValueType::Boolean));
            }

            // Most remaining opcodes are treated as unknown/no-op for typing purposes.
            _ => {}
        }
    }
}

fn reverse_top(stack: &mut [StackValue], count: usize) {
    if count == 0 || stack.len() < count {
        return;
    }
    let start = stack.len() - count;
    stack[start..].reverse();
}

fn pop_or_unknown(stack: &mut Vec<StackValue>) -> StackValue {
    stack.pop().unwrap_or_else(StackValue::unknown)
}

fn push_slot(stack: &mut Vec<StackValue>, ty: Option<ValueType>) {
    stack.push(StackValue::with_type(ty.unwrap_or(ValueType::Unknown)));
}

fn push_indexed_slot(
    stack: &mut Vec<StackValue>,
    slots: &mut Vec<ValueType>,
    operand: Option<&Operand>,
) {
    let Some(Operand::U8(index)) = operand else {
        stack.push(StackValue::unknown());
        return;
    };
    let idx = *index as usize;
    if idx >= slots.len() {
        slots.resize(idx + 1, ValueType::Unknown);
    }
    push_slot(stack, slots.get(idx).copied());
}

fn store_slot(stack: &mut Vec<StackValue>, slots: &mut Vec<ValueType>, index: usize) {
    let value = pop_or_unknown(stack);
    if index >= slots.len() {
        slots.resize(index + 1, ValueType::Unknown);
    }
    let current = slots[index];
    slots[index] = current.join(value.ty);
}

fn store_indexed_slot(
    stack: &mut Vec<StackValue>,
    slots: &mut Vec<ValueType>,
    operand: Option<&Operand>,
) {
    let Some(Operand::U8(index)) = operand else {
        let _ = pop_or_unknown(stack);
        return;
    };
    store_slot(stack, slots, *index as usize);
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

fn int_literal_from_operand(operand: Option<&Operand>) -> Option<i64> {
    match operand {
        Some(Operand::I8(v)) => Some(*v as i64),
        Some(Operand::I16(v)) => Some(*v as i64),
        Some(Operand::I32(v)) => Some(*v as i64),
        Some(Operand::I64(v)) => Some(*v),
        Some(Operand::U8(v)) => Some(*v as i64),
        Some(Operand::U16(v)) => Some(*v as i64),
        Some(Operand::U32(v)) => Some(*v as i64),
        _ => None,
    }
}

fn type_from_manifest(kind: &str) -> ValueType {
    match kind.to_ascii_lowercase().as_str() {
        "any" => ValueType::Any,
        "boolean" => ValueType::Boolean,
        "integer" => ValueType::Integer,
        "string" => ValueType::ByteString,
        "bytearray" => ValueType::ByteString,
        "signature" => ValueType::ByteString,
        "hash160" => ValueType::ByteString,
        "hash256" => ValueType::ByteString,
        "array" => ValueType::Array,
        "map" => ValueType::Map,
        "interopinterface" => ValueType::InteropInterface,
        _ => ValueType::Unknown,
    }
}

fn convert_target_type(operand: &Operand) -> Option<ValueType> {
    let byte = match operand {
        Operand::U8(v) => *v,
        Operand::I8(v) => *v as u8,
        _ => return None,
    };
    match byte {
        0x00 => Some(ValueType::Any),
        0x10 => Some(ValueType::Pointer),
        0x20 => Some(ValueType::Boolean),
        0x21 => Some(ValueType::Integer),
        0x28 => Some(ValueType::ByteString),
        0x30 => Some(ValueType::Buffer),
        0x40 => Some(ValueType::Array),
        0x41 => Some(ValueType::Struct),
        0x48 => Some(ValueType::Map),
        0x60 => Some(ValueType::InteropInterface),
        _ => None,
    }
}

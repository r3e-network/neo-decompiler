//! Lightweight type inference for lifted Neo N3 bytecode.
//!
//! The Neo VM is dynamically typed and most syscalls do not encode argument
//! signatures in the bytecode. The goal of this module is therefore to provide
//! a best-effort type recovery pass that is:
//!
//! - conservative (falls back to `unknown`/`any` rather than guessing)
//! - useful for collection recovery and readability improvements
//! - deterministic and panic-free on malformed input

// Stack depth → i64 and type-tag byte reinterpretation casts are structurally
// safe: stack depth fits in i64, and the i8→u8 cast is intentional.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use std::fmt;

use serde::Serialize;

use crate::decompiler::helpers::value_type_from_operand;
use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::ContractManifest;
use crate::syscalls;

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

impl fmt::Display for ValueType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Unknown => "unknown",
            Self::Any => "any",
            Self::Null => "null",
            Self::Boolean => "boolean",
            Self::Integer => "integer",
            Self::ByteString => "bytestring",
            Self::Buffer => "buffer",
            Self::Array => "array",
            Self::Struct => "struct",
            Self::Map => "map",
            Self::InteropInterface => "interopinterface",
            Self::Pointer => "pointer",
        };
        formatter.write_str(name)
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
    // `instructions` is sorted by offset and `table.spans()` is sorted by start
    // with contiguous ranges, so sweep a single forward cursor instead of
    // re-filtering the whole instruction stream per span (O(instructions *
    // spans), quadratic on call-dense bytecode). See build_xrefs.
    let mut cursor = 0usize;
    for span in table.spans() {
        while cursor < instructions.len() && instructions[cursor].offset < span.start {
            cursor += 1;
        }
        let begin = cursor;
        while cursor < instructions.len() && instructions[cursor].offset < span.end {
            cursor += 1;
        }
        let slice: Vec<&Instruction> = instructions[begin..cursor].iter().collect();

        let (locals_count, args_count) = scan_slot_counts(&slice).unwrap_or((0, 0));
        let mut locals = vec![ValueType::Unknown; locals_count];
        let mut arguments = vec![ValueType::Unknown; args_count];

        if let Some(manifest) = manifest {
            if let Some(index) = table.manifest_index_for_start(span.start) {
                if let Some(method) = manifest.abi.methods.get(index) {
                    if arguments.len() < method.parameters.len() {
                        arguments.resize(method.parameters.len(), ValueType::Unknown);
                    }
                    for (idx, param) in method.parameters.iter().enumerate() {
                        arguments[idx] = arguments[idx].join(type_from_manifest(&param.kind));
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

mod infer;
use infer::{infer_types_in_slice, scan_slot_counts, scan_static_slot_count, type_from_manifest};

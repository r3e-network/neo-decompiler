use std::collections::{BTreeMap, BTreeSet};

use crate::instruction::{Instruction, Operand};

mod control_flow;
mod core;
mod dispatch;
mod postprocess;
mod slots;
mod stack;

fn literal_from_operand(operand: Option<&Operand>) -> Option<LiteralValue> {
    match operand {
        Some(Operand::I8(v)) => Some(LiteralValue::Integer(*v as i64)),
        Some(Operand::I16(v)) => Some(LiteralValue::Integer(*v as i64)),
        Some(Operand::I32(v)) => Some(LiteralValue::Integer(*v as i64)),
        Some(Operand::I64(v)) => Some(LiteralValue::Integer(*v)),
        Some(Operand::U8(v)) => Some(LiteralValue::Integer(*v as i64)),
        Some(Operand::U16(v)) => Some(LiteralValue::Integer(*v as i64)),
        Some(Operand::U32(v)) => Some(LiteralValue::Integer(*v as i64)),
        Some(Operand::Bool(v)) => Some(LiteralValue::Boolean(*v)),
        _ => None,
    }
}

fn convert_target_name(operand: &Operand) -> Option<&'static str> {
    let byte = match operand {
        Operand::U8(v) => *v,
        Operand::I8(v) => *v as u8,
        _ => return None,
    };

    match byte {
        0x00 => Some("any"),
        0x10 => Some("pointer"),
        0x20 => Some("bool"),
        0x21 => Some("integer"),
        0x28 => Some("bytestring"),
        0x30 => Some("buffer"),
        0x40 => Some("array"),
        0x41 => Some("struct"),
        0x48 => Some("map"),
        0x60 => Some("interopinterface"),
        _ => None,
    }
}

#[derive(Debug, Default)]
pub(crate) struct HighLevelEmitter {
    stack: Vec<String>,
    statements: Vec<String>,
    next_temp: usize,
    pending_closers: BTreeMap<usize, usize>,
    else_targets: BTreeMap<usize, usize>,
    #[allow(dead_code)] // Reserved for if-else-if chain detection (planned enhancement)
    else_if_targets: BTreeMap<usize, usize>,
    pending_if_headers: BTreeMap<usize, Vec<String>>,
    catch_targets: BTreeMap<usize, usize>,
    finally_targets: BTreeMap<usize, usize>,
    skip_jumps: BTreeSet<usize>,
    program: Vec<Instruction>,
    index_by_offset: BTreeMap<usize, usize>,
    do_while_headers: BTreeMap<usize, Vec<DoWhileLoop>>,
    active_do_while_tails: BTreeSet<usize>,
    loop_stack: Vec<LoopContext>,
    initialized_locals: BTreeSet<usize>,
    initialized_statics: BTreeSet<usize>,
    argument_labels: BTreeMap<usize, String>,
    literal_values: BTreeMap<String, LiteralValue>,
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

#[derive(Clone, Debug, PartialEq)]
enum LiteralValue {
    Integer(i64),
    Boolean(bool),
}

impl HighLevelEmitter {
    fn note(&mut self, instruction: &Instruction, message: &str) {
        self.statements
            .push(format!("// {:04X}: {}", instruction.offset, message));
    }

    fn stack_underflow(&mut self, instruction: &Instruction, needed: usize) {
        self.statements.push(format!(
            "// {:04X}: insufficient values on stack for {} (needs {needed})",
            instruction.offset, instruction.opcode
        ));
    }

    fn push_comment(&mut self, instruction: &Instruction) {
        self.statements.push(format!(
            "// {:04X}: {}",
            instruction.offset, instruction.opcode
        ));
    }

    fn next_temp(&mut self) -> String {
        let name = format!("t{}", self.next_temp);
        self.next_temp += 1;
        name
    }

    fn take_usize_literal(&mut self, name: &str) -> Option<usize> {
        match self.literal_values.remove(name) {
            Some(LiteralValue::Integer(value)) => usize::try_from(value).ok(),
            _ => None,
        }
    }
}

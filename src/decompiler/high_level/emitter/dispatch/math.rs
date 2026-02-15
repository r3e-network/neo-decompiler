use crate::instruction::{Instruction, OpCode};

use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(super) fn try_emit_math(&mut self, instruction: &Instruction) -> bool {
        use OpCode::*;

        match instruction.opcode {
            Add => self.binary_op(instruction, "+"),
            Sub => self.binary_op(instruction, "-"),
            Mul => self.binary_op(instruction, "*"),
            Div => self.binary_op(instruction, "/"),
            Mod => self.binary_op(instruction, "%"),
            Pow => self.binary_op(instruction, "pow"),
            Sqrt => self.unary_op(instruction, |val| format!("sqrt({val})")),
            Modmul => self.emit_call(instruction, "modmul", 3, true),
            Modpow => self.emit_call(instruction, "modpow", 3, true),
            And => self.binary_op(instruction, "&"),
            Or => self.binary_op(instruction, "|"),
            Xor => self.binary_op(instruction, "^"),
            Shl => self.binary_op(instruction, "<<"),
            Shr => self.binary_op(instruction, ">>"),
            Not => self.unary_op(instruction, |val| format!("!{val}")),
            Nz => self.unary_op(instruction, |val| format!("{val} != 0")),
            Inc => self.unary_op(instruction, |val| format!("{val} + 1")),
            Dec => self.unary_op(instruction, |val| format!("{val} - 1")),
            Negate => self.unary_op(instruction, |val| format!("-{val}")),
            Abs => self.unary_op(instruction, |val| format!("abs({val})")),
            Sign => self.unary_op(instruction, |val| format!("sign({val})")),
            Equal | Numequal => self.binary_op(instruction, "=="),
            Notequal | Numnotequal => self.binary_op(instruction, "!="),
            Gt => self.binary_op(instruction, ">"),
            Ge => self.binary_op(instruction, ">="),
            Lt => self.binary_op(instruction, "<"),
            Le => self.binary_op(instruction, "<="),
            Booland => self.binary_op(instruction, "&&"),
            Boolor => self.binary_op(instruction, "||"),
            Min => self.binary_op(instruction, "min"),
            Max => self.binary_op(instruction, "max"),
            Within => self.emit_call(instruction, "within", 3, true),
            Cat => self.binary_op(instruction, "cat"),
            Substr => self.emit_call(instruction, "substr", 3, true),
            Left => self.binary_op(instruction, "left"),
            Right => self.binary_op(instruction, "right"),
            Invert => self.unary_op(instruction, |val| format!("~{val}")),
            _ => return false,
        }

        true
    }
}

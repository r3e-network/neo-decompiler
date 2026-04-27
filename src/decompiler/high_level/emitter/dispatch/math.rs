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
            // Function-call form (`pow(base, exp)`) matches the JS port
            // and reads as a real helper invocation rather than the
            // pseudo-operator `base pow exp` shape.
            Pow => self.emit_call(instruction, "pow", 2, true),
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
            // Function-call form (`min(a, b)` / `max(a, b)`) — matches
            // the JS port and reads as a helper invocation rather than
            // the pseudo-operator `a min b` shape.
            Min => self.emit_call(instruction, "min", 2, true),
            Max => self.emit_call(instruction, "max", 2, true),
            Within => self.emit_call(instruction, "within", 3, true),
            Cat => self.binary_op(instruction, "cat"),
            Substr => self.emit_call(instruction, "substr", 3, true),
            // Use function-call form (`left(s, n)` / `right(s, n)`) over
            // a pseudo-operator `s left n` shape: matches the JS port
            // and reads naturally as a helper invocation.
            Left => self.emit_call(instruction, "left", 2, true),
            Right => self.emit_call(instruction, "right", 2, true),
            Invert => self.unary_op(instruction, |val| format!("~{val}")),
            _ => return false,
        }

        true
    }
}

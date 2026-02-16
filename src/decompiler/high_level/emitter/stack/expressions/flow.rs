use crate::instruction::{Instruction, Operand};

use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(in super::super::super) fn emit_return(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if let Some(value) = self.pop_stack_value() {
            self.statements.push(format!("return {value};"));
        } else {
            self.statements.push("return;".into());
        }
        self.stack.clear();
    }

    pub(in super::super::super) fn emit_syscall(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if let Some(Operand::Syscall(hash)) = instruction.operand {
            let info = crate::syscalls::lookup(hash);
            // When we do not have metadata, assume the syscall returns a value.
            // This is conservative (avoids stack underflow) and matches Neo's
            // "unknown syscalls push an item" convention.
            let returns_value = info.map(|i| i.returns_value).unwrap_or(true);
            let param_count = info.map(|i| i.param_count).unwrap_or(0) as usize;

            // Pop arguments from the stack (rightmost argument was pushed last).
            let mut args: Vec<String> = Vec::with_capacity(param_count);
            for _ in 0..param_count {
                args.push(self.pop_stack_value().unwrap_or_else(|| "???".into()));
            }
            args.reverse();
            let arg_list = args.join(", ");

            if let Some(info) = info {
                let call = if arg_list.is_empty() {
                    format!("syscall(\"{}\")", info.name)
                } else {
                    format!("syscall(\"{}\", {})", info.name, arg_list)
                };
                if returns_value {
                    let temp = self.next_temp();
                    self.statements
                        .push(format!("let {temp} = {call}; // 0x{hash:08X}"));
                    self.stack.push(temp);
                } else {
                    self.statements.push(format!("{call}; // 0x{hash:08X}"));
                }
            } else {
                let call = format!("syscall(0x{hash:08X})");
                if returns_value {
                    let temp = self.next_temp();
                    self.statements
                        .push(format!("let {temp} = {call}; // unknown syscall"));
                    self.stack.push(temp);
                } else {
                    self.statements.push(format!("{call}; // unknown syscall"));
                }
            }
        } else {
            self.statements.push(format!(
                "// {:04X}: missing syscall operand",
                instruction.offset
            ));
        }
    }
}

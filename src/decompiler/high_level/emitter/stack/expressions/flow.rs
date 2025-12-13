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

            if let Some(info) = info {
                if returns_value {
                    let temp = self.next_temp();
                    self.statements.push(format!(
                        "let {temp} = syscall(\"{}\"); // 0x{hash:08X}",
                        info.name
                    ));
                    self.stack.push(temp);
                } else {
                    self.statements
                        .push(format!("syscall(\"{}\"); // 0x{hash:08X}", info.name));
                }
            } else if returns_value {
                let temp = self.next_temp();
                self.statements.push(format!(
                    "let {temp} = syscall(0x{hash:08X}); // unknown syscall"
                ));
                self.stack.push(temp);
            } else {
                self.statements
                    .push(format!("syscall(0x{hash:08X}); // unknown syscall"));
            }
        } else {
            self.statements.push(format!(
                "// {:04X}: missing syscall operand",
                instruction.offset
            ));
        }
    }
}

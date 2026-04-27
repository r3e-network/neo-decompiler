use crate::instruction::{Instruction, OpCode, Operand};

use super::super::super::{HighLevelEmitter, SlotKind};

impl HighLevelEmitter {
    pub(in super::super::super) fn emit_return(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if self.returns_void {
            // Void method: discard any leftover stack values.
            self.statements.push("return;".into());
        } else if let Some(value) = self.pop_stack_value() {
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
            let syscall_name = info.map(|i| i.name).unwrap_or("unknown syscall");

            // Pop arguments from the stack (rightmost argument was pushed last).
            let mut args: Vec<String> = Vec::with_capacity(param_count);
            let mut missing_argument = false;
            for _ in 0..param_count {
                match self.pop_stack_value() {
                    Some(value) => args.push(value),
                    None => {
                        missing_argument = true;
                        args.push("???".into());
                    }
                }
            }
            if missing_argument {
                let mut message =
                    format!("missing syscall argument values for {syscall_name} (substituted ???)");
                if let Some(context) =
                    self.missing_syscall_argument_context(instruction, syscall_name)
                {
                    message.push_str("; ");
                    message.push_str(&context);
                }
                // Use `warn(...)` so the inline `// XXXX: missing syscall
                // argument values for Foo (substituted ???)` comment fires
                // regardless of trace mode — earlier this delegated to
                // `note(...)` which gated on `emit_trace_comments`, so a
                // reader of the clean-mode rendering saw `???` placeholders
                // with no inline explanation. The JS port has always
                // emitted the comment unconditionally.
                self.warn(instruction, &message);
            }
            args.reverse();
            let arg_list = args.join(", ");

            if let Some(info) = info {
                let call = if arg_list.is_empty() {
                    format!("syscall(\"{}\")", info.name)
                } else {
                    format!("syscall(\"{}\", {})", info.name, arg_list)
                };
                // For known syscalls the name already identifies the call;
                // the 32-bit hash adds no information and clutters output.
                // Keep it only as a debug aid when trace comments are on.
                let trailing = if self.emit_trace_comments {
                    format!(" // 0x{hash:08X}")
                } else {
                    String::new()
                };
                if returns_value {
                    let temp = self.next_temp();
                    self.statements
                        .push(format!("let {temp} = {call};{trailing}"));
                    self.stack.push(temp);
                } else {
                    self.statements.push(format!("{call};{trailing}"));
                }
            } else {
                // Unknown syscall: the hex is in the call expression
                // itself, so the leading `// warning: unknown syscall
                // 0xHASH` annotation is the only hint at why a raw hash
                // is showing up. Earlier this used a trailing
                // `// unknown syscall` comment which diverged from the
                // JS port's leading-comment style and from the rest of
                // Rust's warn-emitted lines (iteration 97 unified those
                // on the `// warning:` prefix). Route through `warn()`
                // so the inline annotation + structured warnings array
                // entry both fire, byte-identical to JS.
                let call = format!("syscall(0x{hash:08X})");
                self.warn(instruction, &format!("unknown syscall 0x{hash:08X}"));
                if returns_value {
                    let temp = self.next_temp();
                    self.statements.push(format!("let {temp} = {call};"));
                    self.stack.push(temp);
                } else {
                    self.statements.push(format!("{call};"));
                }
            }
        } else {
            self.statements.push(format!(
                "// {:04X}: missing syscall operand",
                instruction.offset
            ));
        }
    }

    fn missing_syscall_argument_context(
        &self,
        instruction: &Instruction,
        syscall_name: &str,
    ) -> Option<String> {
        let &instruction_index = self.index_by_offset.get(&instruction.offset)?;
        let previous = instruction_index
            .checked_sub(1)
            .and_then(|index| self.program.get(index))?;
        let (kind, index) = Self::store_slot_context(previous)?;
        let slot_name = Self::format_slot_label(kind, index);
        let stored_value = if self.packed_values_by_name.contains_key(&slot_name) {
            "a packed value"
        } else {
            "the last produced value"
        };
        Some(format!(
            "preceding {} stored {stored_value} into {slot_name}; no value remains on the evaluation stack before {syscall_name}",
            previous.opcode
        ))
    }

    fn store_slot_context(instruction: &Instruction) -> Option<(SlotKind, usize)> {
        use OpCode::{
            Starg, Starg0, Starg1, Starg2, Starg3, Starg4, Starg5, Starg6, Stloc, Stloc0, Stloc1,
            Stloc2, Stloc3, Stloc4, Stloc5, Stloc6, Stsfld, Stsfld0, Stsfld1, Stsfld2, Stsfld3,
            Stsfld4, Stsfld5, Stsfld6,
        };

        match instruction.opcode {
            Stloc0 => Some((SlotKind::Local, 0)),
            Stloc1 => Some((SlotKind::Local, 1)),
            Stloc2 => Some((SlotKind::Local, 2)),
            Stloc3 => Some((SlotKind::Local, 3)),
            Stloc4 => Some((SlotKind::Local, 4)),
            Stloc5 => Some((SlotKind::Local, 5)),
            Stloc6 => Some((SlotKind::Local, 6)),
            Stloc => Self::operand_slot_index(instruction).map(|index| (SlotKind::Local, index)),
            Starg0 => Some((SlotKind::Argument, 0)),
            Starg1 => Some((SlotKind::Argument, 1)),
            Starg2 => Some((SlotKind::Argument, 2)),
            Starg3 => Some((SlotKind::Argument, 3)),
            Starg4 => Some((SlotKind::Argument, 4)),
            Starg5 => Some((SlotKind::Argument, 5)),
            Starg6 => Some((SlotKind::Argument, 6)),
            Starg => Self::operand_slot_index(instruction).map(|index| (SlotKind::Argument, index)),
            Stsfld0 => Some((SlotKind::Static, 0)),
            Stsfld1 => Some((SlotKind::Static, 1)),
            Stsfld2 => Some((SlotKind::Static, 2)),
            Stsfld3 => Some((SlotKind::Static, 3)),
            Stsfld4 => Some((SlotKind::Static, 4)),
            Stsfld5 => Some((SlotKind::Static, 5)),
            Stsfld6 => Some((SlotKind::Static, 6)),
            Stsfld => Self::operand_slot_index(instruction).map(|index| (SlotKind::Static, index)),
            _ => None,
        }
    }

    fn operand_slot_index(instruction: &Instruction) -> Option<usize> {
        match instruction.operand {
            Some(Operand::U8(value)) => Some(value as usize),
            _ => None,
        }
    }

    fn format_slot_label(kind: SlotKind, index: usize) -> String {
        match kind {
            SlotKind::Local => format!("loc{index}"),
            SlotKind::Argument => format!("arg{index}"),
            SlotKind::Static => format!("static{index}"),
        }
    }
}

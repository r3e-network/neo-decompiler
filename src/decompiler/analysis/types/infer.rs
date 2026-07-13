use super::*;
pub(super) fn infer_types_in_slice(
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

            OpCode::Syscall => {
                let Some(info) = instr.operand.as_ref().and_then(|operand| match operand {
                    Operand::Syscall(hash) => syscalls::lookup(*hash),
                    _ => None,
                }) else {
                    stack.push(StackValue::unknown());
                    continue;
                };
                for _ in 0..info.param_count {
                    let _ = pop_or_unknown(&mut stack);
                }
                if info.returns_value {
                    stack.push(StackValue::unknown());
                }
            }

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
            OpCode::Swap if stack.len() >= 2 => {
                let len = stack.len();
                stack.swap(len - 1, len - 2);
            }
            OpCode::Over => {
                let value = stack
                    .get(stack.len().saturating_sub(2))
                    .copied()
                    .unwrap_or_else(StackValue::unknown);
                stack.push(value);
            }
            OpCode::Nip if stack.len() >= 2 => {
                let len = stack.len();
                stack.remove(len - 2);
            }
            OpCode::Rot => {
                if let (Some(top), Some(mid), Some(bottom)) =
                    (stack.pop(), stack.pop(), stack.pop())
                {
                    stack.push(mid);
                    stack.push(top);
                    stack.push(bottom);
                }
            }
            OpCode::Tuck => {
                if let (Some(top), Some(second)) = (stack.pop(), stack.pop()) {
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
            OpCode::Ldloc0 => push_fixed_slot(&mut stack, locals, 0),
            OpCode::Ldloc1 => push_fixed_slot(&mut stack, locals, 1),
            OpCode::Ldloc2 => push_fixed_slot(&mut stack, locals, 2),
            OpCode::Ldloc3 => push_fixed_slot(&mut stack, locals, 3),
            OpCode::Ldloc4 => push_fixed_slot(&mut stack, locals, 4),
            OpCode::Ldloc5 => push_fixed_slot(&mut stack, locals, 5),
            OpCode::Ldloc6 => push_fixed_slot(&mut stack, locals, 6),
            OpCode::Ldloc => push_indexed_slot(&mut stack, locals, instr.operand.as_ref()),
            OpCode::Stloc0 => store_slot(&mut stack, locals, 0),
            OpCode::Stloc1 => store_slot(&mut stack, locals, 1),
            OpCode::Stloc2 => store_slot(&mut stack, locals, 2),
            OpCode::Stloc3 => store_slot(&mut stack, locals, 3),
            OpCode::Stloc4 => store_slot(&mut stack, locals, 4),
            OpCode::Stloc5 => store_slot(&mut stack, locals, 5),
            OpCode::Stloc6 => store_slot(&mut stack, locals, 6),
            OpCode::Stloc => store_indexed_slot(&mut stack, locals, instr.operand.as_ref()),

            OpCode::Ldarg0 => push_fixed_slot(&mut stack, arguments, 0),
            OpCode::Ldarg1 => push_fixed_slot(&mut stack, arguments, 1),
            OpCode::Ldarg2 => push_fixed_slot(&mut stack, arguments, 2),
            OpCode::Ldarg3 => push_fixed_slot(&mut stack, arguments, 3),
            OpCode::Ldarg4 => push_fixed_slot(&mut stack, arguments, 4),
            OpCode::Ldarg5 => push_fixed_slot(&mut stack, arguments, 5),
            OpCode::Ldarg6 => push_fixed_slot(&mut stack, arguments, 6),
            OpCode::Ldarg => push_indexed_slot(&mut stack, arguments, instr.operand.as_ref()),
            OpCode::Starg0 => store_slot(&mut stack, arguments, 0),
            OpCode::Starg1 => store_slot(&mut stack, arguments, 1),
            OpCode::Starg2 => store_slot(&mut stack, arguments, 2),
            OpCode::Starg3 => store_slot(&mut stack, arguments, 3),
            OpCode::Starg4 => store_slot(&mut stack, arguments, 4),
            OpCode::Starg5 => store_slot(&mut stack, arguments, 5),
            OpCode::Starg6 => store_slot(&mut stack, arguments, 6),
            OpCode::Starg => store_indexed_slot(&mut stack, arguments, instr.operand.as_ref()),

            OpCode::Ldsfld0 => push_fixed_slot(&mut stack, statics, 0),
            OpCode::Ldsfld1 => push_fixed_slot(&mut stack, statics, 1),
            OpCode::Ldsfld2 => push_fixed_slot(&mut stack, statics, 2),
            OpCode::Ldsfld3 => push_fixed_slot(&mut stack, statics, 3),
            OpCode::Ldsfld4 => push_fixed_slot(&mut stack, statics, 4),
            OpCode::Ldsfld5 => push_fixed_slot(&mut stack, statics, 5),
            OpCode::Ldsfld6 => push_fixed_slot(&mut stack, statics, 6),
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
            OpCode::Newarray | OpCode::NewarrayT => {
                let _ = pop_or_unknown(&mut stack); // count (NEWARRAY_T also carries an element-type operand)
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
                    // Clamp to the actual stack depth: the count literal is
                    // attacker-controlled (up to i64::MAX) and popping an
                    // empty abstract stack only yields unknowns, so iterating
                    // past the real depth is an unbounded busy-loop.
                    for _ in 0..count.min(stack.len()) {
                        let _ = pop_or_unknown(&mut stack);
                    }
                }
                stack.push(StackValue::with_type(ValueType::Array));
            }
            OpCode::Packmap => {
                let count = pop_or_unknown(&mut stack);
                if let Some(count) = count.int_literal.and_then(|v| usize::try_from(v).ok()) {
                    // PACKMAP pops a key/value pair per entry (`Pop: 2n+1`,
                    // OpCode.cs); clamp like PACK to bound attacker-controlled
                    // counts.
                    for _ in 0..count.saturating_mul(2).min(stack.len()) {
                        let _ = pop_or_unknown(&mut stack);
                    }
                }
                stack.push(StackValue::with_type(ValueType::Map));
            }
            OpCode::Packstruct => {
                let count = pop_or_unknown(&mut stack);
                if let Some(count) = count.int_literal.and_then(|v| usize::try_from(v).ok()) {
                    // Clamp like PACK to bound attacker-controlled counts.
                    for _ in 0..count.min(stack.len()) {
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
            OpCode::Clearitems | OpCode::Reverseitems => {
                let _ = pop_or_unknown(&mut stack);
            }
            OpCode::Memcpy => {
                for _ in 0..5 {
                    let _ = pop_or_unknown(&mut stack);
                }
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
                stack.push(StackValue::with_type(ValueType::Boolean));
            }
            OpCode::Convert => {
                // CONVERT deterministically yields the target type regardless of
                // the input type, so push the decoded target directly instead of
                // joining with the consumed value's type (which over-widened to
                // Any). Mirrors the JS port. Unknown operands fall back to Any.
                let _ = pop_or_unknown(&mut stack);
                let target = instr
                    .operand
                    .as_ref()
                    .and_then(value_type_from_operand)
                    .unwrap_or(ValueType::Any);
                stack.push(StackValue::with_type(target));
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

fn push_fixed_slot(stack: &mut Vec<StackValue>, slots: &mut Vec<ValueType>, index: usize) {
    if index >= slots.len() {
        slots.resize(index + 1, ValueType::Unknown);
    }
    push_slot(stack, slots.get(index).copied());
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

pub(super) fn scan_slot_counts(instructions: &[&Instruction]) -> Option<(usize, usize)> {
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

pub(super) fn scan_static_slot_count(instructions: &[Instruction]) -> Option<usize> {
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

pub(super) fn type_from_manifest(kind: &str) -> ValueType {
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

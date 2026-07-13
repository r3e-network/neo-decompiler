//! Stack-effect model for every Neo VM opcode.
//!
//! Each fixed-arity opcode has a `(pop, push)` signature describing how many
//! eval-stack values it consumes and produces. Stack-reorder opcodes
//! (DUP/SWAP/OVER/ROT/PICK/NIP/TUCK/ROLL/XDROP/REVERSE*) and a few specials
//! (CLEAR, variable-arity PACK/UNPACK, SYSCALL) are handled by dedicated code
//! in the SSA builder because their effect depends on operand values or
//! transforms the stack non-uniformly.
//!
//! Effects are exhaustive for fixed-arity opcodes so the symbolic stack stays
//! consistent on real bytecode; the builder carries any opcode it does not
//! want to render as an expression as a comment `Other` statement.

use crate::instruction::OpCode;

/// `(number of values popped, number of values pushed)`.
pub(crate) type Effect = (usize, usize);

/// Compute the fixed stack effect of `op`.
///
/// Opcodes reported as `(0, 0)` here are either true no-ops at the stack level
/// (control flow, slot init), or specials whose effect is operand-dependent /
/// non-uniform (reorders, CLEAR, PACK/UNPACK, SYSCALL) and are dispatched by
/// the builder via [`is_stack_reorder`] / [`is_stack_special`].
#[must_use]
pub(crate) fn stack_effect(op: OpCode) -> Effect {
    use OpCode::*;
    match op {
        // --- Push immediates: 0 popped, 1 pushed. ---
        Push0 | Push1 | Push2 | Push3 | Push4 | Push5 | Push6 | Push7 | Push8 | Push9 | Push10
        | Push11 | Push12 | Push13 | Push14 | Push15 | Push16 | PushM1 | Pushint8 | Pushint16
        | Pushint32 | Pushint64 | Pushint128 | Pushint256 | Pushdata1 | Pushdata2 | Pushdata4
        | PushA | PushT | PushF | PushNull => (0, 1),

        // --- Binary compute: pop 2, push 1. ---
        Add | Sub | Mul | Div | Mod | Pow | Shl | Shr | And | Or | Xor | Equal | Notequal | Lt
        | Le | Gt | Ge | Booland | Boolor | Numequal | Numnotequal | Min | Max | Cat => (2, 1),

        // --- Ternary compute: pop 3, push 1. ---
        Within | Substr | Modmul | Modpow => (3, 1),

        // --- Binary byte ops: pop 2, push 1. ---
        Left | Right => (2, 1),

        // --- Unary compute: pop 1, push 1. ---
        Sqrt | Abs | Negate | Inc | Dec | Sign | Not | Nz | Invert | Isnull | Istype | Convert
        | Size | Keys | Values => (1, 1),

        // --- Slot loads: 0 popped, 1 pushed (the loaded value). ---
        Ldsfld0 | Ldsfld1 | Ldsfld2 | Ldsfld3 | Ldsfld4 | Ldsfld5 | Ldsfld6 | Ldsfld | Ldloc0
        | Ldloc1 | Ldloc2 | Ldloc3 | Ldloc4 | Ldloc5 | Ldloc6 | Ldloc | Ldarg0 | Ldarg1
        | Ldarg2 | Ldarg3 | Ldarg4 | Ldarg5 | Ldarg6 | Ldarg => (0, 1),

        // --- Slot stores: 1 popped, 0 pushed. ---
        Stsfld0 | Stsfld1 | Stsfld2 | Stsfld3 | Stsfld4 | Stsfld5 | Stsfld6 | Stsfld | Stloc0
        | Stloc1 | Stloc2 | Stloc3 | Stloc4 | Stloc5 | Stloc6 | Stloc | Starg0 | Starg1
        | Starg2 | Starg3 | Starg4 | Starg5 | Starg6 | Starg => (1, 0),

        // --- Collection constructors. ---
        // Empty: 0 popped, 1 pushed.
        Newarray0 | Newstruct0 | Newmap => (0, 1),
        // Sized: pop 1 (size), push 1.
        Newarray | Newstruct | NewarrayT | Newbuffer => (1, 1),
        // Popitem: pop 1 (container), push 1 (removed item).
        Popitem => (1, 1),

        // --- Collection accessors: pop N, push 1. ---
        Pickitem => (2, 1), // container, index -> item
        Haskey => (2, 1),   // container, key -> bool

        // --- Collection mutators: pop N, push 0. ---
        Append => (2, 0),       // container, item
        Setitem => (3, 0),      // container, key, value
        Remove => (2, 0),       // container, key
        Clearitems => (1, 0),   // container
        Reverseitems => (1, 0), // container
        Memcpy => (5, 0),       // destination, destination index, source, source index, count

        // --- Conditional jumps pop their condition(s): handled as pops so the
        //     symbolic stack stays consistent across branch merges. ---
        Jmpif | Jmpif_L | Jmpifnot | Jmpifnot_L => (1, 0),
        JmpEq | JmpEq_L | JmpNe | JmpNe_L | JmpGt | JmpGt_L | JmpGe | JmpGe_L | JmpLt | JmpLt_L
        | JmpLe | JmpLe_L => (2, 0),

        // --- Asserts / throws consume their operand(s). ---
        Assert | Throw => (1, 0),
        Assertmsg => (2, 0),
        Abortmsg => (1, 0),

        // --- Opaque calls conservatively produce a value. CALLA additionally
        // consumes its function pointer. Precise argument/void metadata is
        // layered on by per-method render contexts when available.
        Call | Call_L | CallT => (0, 1),
        CallA => (1, 1),

        // --- True stack-level no-ops (terminators and slot init). ---
        Nop | Jmp | Jmp_L | Abort | Try | TryL | Endtry | EndtryL | Endfinally | Ret
        | Initsslot | Initslot => (0, 0),

        // --- Reorders / specials: dispatched by the builder. ---
        Depth | Drop | Nip | Dup | Over | Tuck | Swap | Rot | Reverse3 | Reverse4 | Xdrop
        | Pick | Roll | Reversen | Clear | Syscall | Pack | Packmap | Packstruct | Unpack => (0, 0),

        // --- Unknown / future opcodes: neutral (no modelled effect). ---
        Unknown(_) => (0, 0),
    }
}

/// Whether `op` is a fixed-shape stack reorder the builder must transform the
/// symbolic stack for (no consume-and-produce semantics).
#[must_use]
pub(crate) fn is_stack_reorder(op: OpCode) -> bool {
    use OpCode::*;
    matches!(
        op,
        Dup | Over | Tuck | Swap | Rot | Reverse3 | Reverse4 | Depth | Drop | Nip
    )
}

/// Whether `op` is a special whose stack effect is operand-dependent and must
/// be resolved by the builder (PICK/ROLL/XDROP/REVERSEN read an index from the
/// stack; PACK/PACKMAP/PACKSTRUCT/UNPACK read a count; CLEAR empties; SYSCALL
/// reads its arity from the syscall table).
#[must_use]
pub(crate) fn is_stack_special(op: OpCode) -> bool {
    use OpCode::*;
    matches!(
        op,
        Pick | Roll | Xdrop | Reversen | Pack | Packmap | Packstruct | Unpack | Clear | Syscall
    )
}

#[cfg(test)]
mod tests {
    use super::{is_stack_reorder, is_stack_special, stack_effect};
    use crate::instruction::OpCode;

    #[test]
    fn push_opcodes_produce_one_value() {
        for op in [
            OpCode::Push0,
            OpCode::Push1,
            OpCode::Push16,
            OpCode::PushM1,
            OpCode::Pushint64,
            OpCode::Pushdata4,
            OpCode::PushNull,
            OpCode::PushA,
            OpCode::PushT,
            OpCode::PushF,
        ] {
            assert_eq!(stack_effect(op), (0, 1), "{op:?}");
        }
    }

    #[test]
    fn slot_loads_push_one() {
        for op in [
            OpCode::Ldloc0,
            OpCode::Ldloc3,
            OpCode::Ldarg0,
            OpCode::Ldsfld6,
            OpCode::Ldloc,
        ] {
            assert_eq!(stack_effect(op), (0, 1), "{op:?}");
        }
    }

    #[test]
    fn slot_stores_pop_one() {
        for op in [
            OpCode::Stloc0,
            OpCode::Stloc,
            OpCode::Starg2,
            OpCode::Stsfld0,
        ] {
            assert_eq!(stack_effect(op), (1, 0), "{op:?}");
        }
    }

    #[test]
    fn collection_ops_have_correct_effects() {
        assert_eq!(stack_effect(OpCode::Newarray0), (0, 1));
        assert_eq!(stack_effect(OpCode::Newarray), (1, 1));
        assert_eq!(stack_effect(OpCode::Pickitem), (2, 1));
        assert_eq!(stack_effect(OpCode::Append), (2, 0));
        assert_eq!(stack_effect(OpCode::Setitem), (3, 0));
        assert_eq!(stack_effect(OpCode::Remove), (2, 0));
        assert_eq!(stack_effect(OpCode::Clearitems), (1, 0));
        assert_eq!(stack_effect(OpCode::Reverseitems), (1, 0));
        assert_eq!(stack_effect(OpCode::Memcpy), (5, 0));
        assert_eq!(stack_effect(OpCode::Popitem), (1, 1));
    }

    #[test]
    fn conditional_jumps_pop_conditions() {
        assert_eq!(stack_effect(OpCode::Jmpif), (1, 0));
        assert_eq!(stack_effect(OpCode::Jmpifnot_L), (1, 0));
        assert_eq!(stack_effect(OpCode::JmpEq), (2, 0));
        assert_eq!(stack_effect(OpCode::JmpLt_L), (2, 0));
    }

    #[test]
    fn reorders_and_specials_are_neutral_in_the_table() {
        // The table reports (0,0); the builder transforms the symbolic stack
        // directly for these.
        for op in [
            OpCode::Dup,
            OpCode::Swap,
            OpCode::Rot,
            OpCode::Drop,
            OpCode::Depth,
            OpCode::Pick,
            OpCode::Roll,
            OpCode::Pack,
            OpCode::Clear,
            OpCode::Syscall,
        ] {
            assert_eq!(stack_effect(op), (0, 0), "{op:?}");
            assert!(
                is_stack_reorder(op) || is_stack_special(op),
                "{op:?} should be classified as reorder or special"
            );
        }
    }

    #[test]
    fn control_flow_is_neutral_and_calls_produce_values() {
        for op in [
            OpCode::Nop,
            OpCode::Jmp,
            OpCode::Ret,
            OpCode::Initslot,
            OpCode::Try,
            OpCode::Endfinally,
        ] {
            assert_eq!(stack_effect(op), (0, 0), "{op:?}");
            assert!(!is_stack_reorder(op));
            assert!(!is_stack_special(op));
        }
        assert_eq!(stack_effect(OpCode::Call), (0, 1));
        assert_eq!(stack_effect(OpCode::Call_L), (0, 1));
        assert_eq!(stack_effect(OpCode::CallT), (0, 1));
        assert_eq!(stack_effect(OpCode::CallA), (1, 1));
    }
}

use crate::decompiler::cfg::method_body::LoweringIssueKind;
use crate::instruction::OpCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpcodeFidelity {
    Exact,
    Conservative,
    Incomplete(LoweringIssueKind),
}

pub(crate) fn classify_opcode(opcode: OpCode) -> OpcodeFidelity {
    use OpCode::*;

    match opcode {
        Unknown(_) => OpcodeFidelity::Incomplete(LoweringIssueKind::UnsupportedOpcode),
        Xdrop | Pick | Roll | Reversen => OpcodeFidelity::Exact,
        Abort | Abortmsg | Syscall => OpcodeFidelity::Conservative,
        Pushint8 | Pushint16 | Pushint32 | Pushint64 | Pushint128 | Pushint256 | PushT | PushF
        | PushA | PushNull | Pushdata1 | Pushdata2 | Pushdata4 | PushM1 | Push0 | Push1 | Push2
        | Push3 | Push4 | Push5 | Push6 | Push7 | Push8 | Push9 | Push10 | Push11 | Push12
        | Push13 | Push14 | Push15 | Push16 | Nop | Jmp | Jmp_L | Jmpif | Jmpif_L | Jmpifnot
        | Jmpifnot_L | JmpEq | JmpEq_L | JmpNe | JmpNe_L | JmpGt | JmpGt_L | JmpGe | JmpGe_L
        | JmpLt | JmpLt_L | JmpLe | JmpLe_L | Call | Call_L | CallA | CallT | Ret | Depth
        | Drop | Nip | Clear | Dup | Over | Tuck | Swap | Rot | Reverse3 | Reverse4 | Initsslot
        | Initslot | Ldsfld0 | Ldsfld1 | Ldsfld2 | Ldsfld3 | Ldsfld4 | Ldsfld5 | Ldsfld6
        | Ldsfld | Stsfld0 | Stsfld1 | Stsfld2 | Stsfld3 | Stsfld4 | Stsfld5 | Stsfld6 | Stsfld
        | Ldloc0 | Ldloc1 | Ldloc2 | Ldloc3 | Ldloc4 | Ldloc5 | Ldloc6 | Ldloc | Stloc0
        | Stloc1 | Stloc2 | Stloc3 | Stloc4 | Stloc5 | Stloc6 | Stloc | Ldarg0 | Ldarg1
        | Ldarg2 | Ldarg3 | Ldarg4 | Ldarg5 | Ldarg6 | Ldarg | Starg0 | Starg1 | Starg2
        | Starg3 | Starg4 | Starg5 | Starg6 | Starg | Newbuffer | Memcpy | Cat | Substr | Left
        | Right | Invert | And | Or | Xor | Equal | Notequal | Sign | Abs | Negate | Inc | Dec
        | Add | Sub | Mul | Div | Mod | Pow | Sqrt | Modmul | Modpow | Shl | Shr | Not
        | Booland | Boolor | Nz | Numequal | Numnotequal | Lt | Le | Gt | Ge | Min | Max
        | Within | Newarray0 | Newarray | NewarrayT | Newstruct0 | Newstruct | Newmap | Pack
        | Packmap | Packstruct | Unpack | Size | Haskey | Keys | Values | Pickitem | Append
        | Setitem | Reverseitems | Remove | Clearitems | Popitem | Isnull | Istype | Convert
        | Assert | Assertmsg | Throw | Try | TryL | Endtry | EndtryL | Endfinally => {
            OpcodeFidelity::Exact
        }
    }
}

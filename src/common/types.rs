//! Core type definitions for the Neo N3 decompiler

use serde::{Deserialize, Serialize};

/// Neo N3 VM instruction representation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Instruction {
    /// Bytecode offset
    pub offset: u32,
    /// Instruction opcode
    pub opcode: OpCode,
    /// Operand data
    pub operand: Option<Operand>,
    /// Size in bytes
    pub size: u8,
}

/// Neo N3 opcodes enumeration - Complete N3 VM instruction set
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OpCode {
    // Constants (0x00-0x20)
    PUSHINT8,
    PUSHINT16,
    PUSHINT32,
    PUSHINT64,
    PUSHINT128,
    PUSHINT256,
    PUSHT,
    PUSHF,
    PUSHA,
    PUSHNULL,
    PUSHDATA1,
    PUSHDATA2,
    PUSHDATA4,
    PUSHM1,
    PUSH0,
    PUSH1,
    PUSH2,
    PUSH3,
    PUSH4,
    PUSH5,
    PUSH6,
    PUSH7,
    PUSH8,
    PUSH9,
    PUSH10,
    PUSH11,
    PUSH12,
    PUSH13,
    PUSH14,
    PUSH15,
    PUSH16,

    // Flow Control (0x21-0x41)
    NOP,
    JMP,
    JMP_L,
    JMPIF,
    JMPIF_L,
    JMPIFNOT,
    JMPIFNOT_L,
    JMPEQ,
    JMPEQ_L,
    JMPNE,
    JMPNE_L,
    JMPGT,
    JMPGT_L,
    JMPGE,
    JMPGE_L,
    JMPLT,
    JMPLT_L,
    JMPLE,
    JMPLE_L,
    CALL,
    CALL_L,
    CALLA,
    CALLT,
    ABORT,
    ASSERT,
    THROW,
    TRY,
    TRY_L,
    ENDTRY,
    ENDTRY_L,
    ENDFINALLY,
    RET,
    SYSCALL,

    // Stack operations (0x43-0x55)
    DEPTH,
    DROP,
    NIP,
    XDROP,
    CLEAR,
    DUP,
    OVER,
    PICK,
    TUCK,
    SWAP,
    ROT,
    ROLL,
    REVERSE3,
    REVERSE4,
    REVERSEN,
    DUP2,

    // Slot operations (0x56-0x87)
    INITSSLOT,
    INITSLOT,
    LDSFLD0,
    LDSFLD1,
    LDSFLD2,
    LDSFLD3,
    LDSFLD4,
    LDSFLD5,
    LDSFLD6,
    LDSFLD,
    STSFLD,
    LDLOC0,
    LDLOC1,
    LDLOC2,
    LDLOC3,
    LDLOC4,
    LDLOC5,
    LDLOC6,
    LDLOC,
    STLOC,
    LDARG0,
    LDARG1,
    LDARG2,
    LDARG3,
    LDARG4,
    LDARG5,
    LDARG6,
    LDARG,
    STARG,
    STARG0,
    STARG1,
    STARG2,
    STARG3,
    STARG4,
    STARG5,
    STARG6,

    // Splice operations (0x88-0x8E)
    NEWBUFFER,
    MEMCPY,
    CAT,
    SUBSTR,
    LEFT,
    RIGHT,
    SIZE,

    // Bitwise logic (0x90-0x98)
    INVERT,
    AND,
    OR,
    XOR,
    EQUAL,
    NOTEQUAL,
    SIGN,
    ABS,
    NEGATE,

    // Arithmetic (0x99-0xBB)
    INC,
    DEC,
    ADD,
    SUB,
    MUL,
    DIV,
    MOD,
    POW,
    SQRT,
    MODMUL,
    MODPOW,
    SHL,
    SHR,
    NOT,
    BOOLAND,
    BOOLOR,
    NZ,
    NUMEQUAL,
    NUMNOTEQUAL,
    LT,
    LE,
    GT,
    GE,
    MIN,
    MAX,
    WITHIN,

    // Additional Neo N3 opcodes found in contracts
    UNKNOWN_07,
    UNKNOWN_42,
    UNKNOWN_44,
    UNKNOWN_B6,
    UNKNOWN_B7,
    UNKNOWN_B8,
    UNKNOWN_BB,
    UNKNOWN_94,
    UNKNOWN_DA,
    UNKNOWN_E4,
    UNKNOWN_E6,
    UNKNOWN_E8,
    UNKNOWN_E9,
    UNKNOWN_EA,
    UNKNOWN_EC,
    UNKNOWN_EF,
    UNKNOWN_F0,
    UNKNOWN_F2,
    UNKNOWN_F7,
    UNKNOWN_FF,

    // Compound types (0xBE-0xD4)
    PACKMAP,
    PACKSTRUCT,
    PACKARRAY,
    PACK,
    UNPACK,
    NEWARRAY0,
    NEWARRAY,
    NEWARRAYT,
    NEWSTRUCT0,
    NEWSTRUCT,
    NEWMAP,
    APPEND,
    SETITEM,
    PICKITEM,
    REMOVE,
    CLEARITEMS,
    POPITEM,
    HASKEY,
    KEYS,
    VALUES,
    SLICE,

    // Types (0xD8-0xDB)
    ISNULL,
    ISTYPE,
    CONVERT,

    // Extensions (0xE0-0xE1)
    ABORTMSG,
    ASSERTMSG,

    // Custom/Unknown
    UNKNOWN(u8),
}

impl OpCode {
    /// Convert byte value to opcode
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            // Constants (0x00-0x20)
            0x00 => OpCode::PUSHINT8,
            0x01 => OpCode::PUSHINT16,
            0x02 => OpCode::PUSHINT32,
            0x03 => OpCode::PUSHINT64,
            0x04 => OpCode::PUSHINT128,
            0x05 => OpCode::PUSHINT256,
            0x08 => OpCode::PUSHT,
            0x09 => OpCode::PUSHF,
            0x0A => OpCode::PUSHA,
            0x0B => OpCode::PUSHNULL,
            0x0C => OpCode::PUSHDATA1,
            0x0D => OpCode::PUSHDATA2,
            0x0E => OpCode::PUSHDATA4,
            0x0F => OpCode::PUSHM1,
            0x10 => OpCode::PUSH0,
            0x11 => OpCode::PUSH1,
            0x12 => OpCode::PUSH2,
            0x13 => OpCode::PUSH3,
            0x14 => OpCode::PUSH4,
            0x15 => OpCode::PUSH5,
            0x16 => OpCode::PUSH6,
            0x17 => OpCode::PUSH7,
            0x18 => OpCode::PUSH8,
            0x19 => OpCode::PUSH9,
            0x1A => OpCode::PUSH10,
            0x1B => OpCode::PUSH11,
            0x1C => OpCode::PUSH12,
            0x1D => OpCode::PUSH13,
            0x1E => OpCode::PUSH14,
            0x1F => OpCode::PUSH15,
            0x20 => OpCode::PUSH16,

            // Flow Control (0x21-0x41)
            0x21 => OpCode::NOP,
            0x22 => OpCode::JMP,
            0x23 => OpCode::JMP_L,
            0x24 => OpCode::JMPIF,
            0x25 => OpCode::JMPIF_L,
            0x26 => OpCode::JMPIFNOT,
            0x27 => OpCode::JMPIFNOT_L,
            0x28 => OpCode::JMPEQ,
            0x29 => OpCode::JMPEQ_L,
            0x2A => OpCode::JMPNE,
            0x2B => OpCode::JMPNE_L,
            0x2C => OpCode::JMPGT,
            0x2D => OpCode::JMPGT_L,
            0x2E => OpCode::JMPGE,
            0x2F => OpCode::JMPGE_L,
            0x30 => OpCode::JMPLT,
            0x31 => OpCode::JMPLT_L,
            0x32 => OpCode::JMPLE,
            0x33 => OpCode::JMPLE_L,
            0x34 => OpCode::CALL,
            0x35 => OpCode::CALL_L,
            0x36 => OpCode::CALLA,
            0x37 => OpCode::CALLT,
            0x38 => OpCode::ABORT,
            0x39 => OpCode::ASSERT,
            0x3A => OpCode::THROW,
            0x3B => OpCode::TRY,
            0x3C => OpCode::TRY_L,
            0x3D => OpCode::ENDTRY,
            0x3E => OpCode::ENDTRY_L,
            0x3F => OpCode::ENDFINALLY,
            0x40 => OpCode::RET,
            0x41 => OpCode::SYSCALL,

            // Stack operations (0x43-0x55)
            0x43 => OpCode::DEPTH,
            0x45 => OpCode::DROP,
            0x46 => OpCode::NIP,
            0x48 => OpCode::XDROP,
            0x49 => OpCode::CLEAR,
            0x4A => OpCode::DUP,
            0x4B => OpCode::OVER,
            0x4C => OpCode::DUP2,
            0x4D => OpCode::PICK,
            0x4E => OpCode::TUCK,
            0x50 => OpCode::SWAP,
            0x51 => OpCode::ROT,
            0x52 => OpCode::ROLL,
            0x53 => OpCode::REVERSE3,
            0x54 => OpCode::REVERSE4,
            0x55 => OpCode::REVERSEN,

            // Slot operations (0x56-0x87)
            0x56 => OpCode::INITSSLOT,
            0x57 => OpCode::INITSLOT,
            0x58 => OpCode::LDSFLD0,
            0x59 => OpCode::LDSFLD1,
            0x5A => OpCode::LDSFLD2,
            0x5B => OpCode::LDSFLD3,
            0x5C => OpCode::LDSFLD4,
            0x5D => OpCode::LDSFLD5,
            0x5E => OpCode::LDSFLD6,
            0x5F => OpCode::LDSFLD,
            0x60 => OpCode::STSFLD,
            0x61 => OpCode::LDLOC0,
            0x62 => OpCode::LDLOC1,
            0x63 => OpCode::LDLOC2,
            0x64 => OpCode::LDLOC3,
            0x65 => OpCode::LDLOC4,
            0x66 => OpCode::LDLOC5,
            0x67 => OpCode::LDLOC6,
            0x68 => OpCode::LDLOC,
            0x69 => OpCode::STLOC,
            0x6A => OpCode::LDARG0,
            0x6B => OpCode::LDARG1,
            0x6C => OpCode::LDARG2,
            0x6D => OpCode::LDARG3,
            0x6E => OpCode::LDARG4,
            0x6F => OpCode::LDARG5,
            0x70 => OpCode::LDARG6,
            0x71 => OpCode::LDARG,
            0x72 => OpCode::STARG,
            0x73 => OpCode::STARG0,
            0x74 => OpCode::STARG1,
            0x75 => OpCode::STARG2,
            0x76 => OpCode::STARG3,
            0x77 => OpCode::STARG4,
            0x78 => OpCode::STARG5,
            0x79 => OpCode::STARG6,
            0x7A => OpCode::STARG,  // Alternative STARG
            0x7B => OpCode::STARG,  // Alternative STARG
            0x7C => OpCode::STARG,  // Alternative STARG
            0x7D => OpCode::STARG,  // Alternative STARG
            0x7E => OpCode::STARG,  // Alternative STARG
            0x7F => OpCode::STARG,  // Alternative STARG
            0x80 => OpCode::STARG0, // Official Neo N3 STARG0 mapping
            0x81 => OpCode::STARG1,
            0x82 => OpCode::STARG2,
            0x83 => OpCode::STARG3,
            0x84 => OpCode::STARG4,
            0x85 => OpCode::STARG5,
            0x86 => OpCode::STARG6,
            0x87 => OpCode::STARG, // Generic STARG

            // Splice operations (0x88-0x8E)
            0x88 => OpCode::NEWBUFFER,
            0x89 => OpCode::MEMCPY,
            0x8B => OpCode::CAT,
            0x8C => OpCode::SUBSTR,
            0x8D => OpCode::LEFT,
            0x8E => OpCode::RIGHT,
            0x8F => OpCode::SIZE, // Official Neo N3 SIZE mapping

            // Bitwise logic (0x90-0x98)
            0x90 => OpCode::INVERT,
            0x91 => OpCode::AND,
            0x92 => OpCode::OR,
            0x93 => OpCode::XOR,
            0x97 => OpCode::EQUAL,
            0x98 => OpCode::NOTEQUAL,

            // Arithmetic (0x99-0xBB)
            0x99 => OpCode::SIGN,
            0x9A => OpCode::ABS,
            0x9B => OpCode::NEGATE,
            0x9C => OpCode::INC,
            0x9D => OpCode::DEC,
            0x9E => OpCode::ADD,
            0x9F => OpCode::SUB,
            0xA0 => OpCode::MUL,
            0xA1 => OpCode::DIV,
            0xA2 => OpCode::MOD,
            0xA3 => OpCode::POW,
            0xA4 => OpCode::SQRT,
            0xA5 => OpCode::MODMUL,
            0xA6 => OpCode::MODPOW,
            0xA7 => OpCode::SHL,
            0xA8 => OpCode::SHR,
            0xA9 => OpCode::NOT,
            0xAA => OpCode::BOOLAND,
            0xAB => OpCode::BOOLOR,
            0xAC => OpCode::NZ,
            0xAD => OpCode::NUMEQUAL,
            0xAE => OpCode::NUMNOTEQUAL,
            0xAF => OpCode::LT,
            0xB0 => OpCode::LE,
            0xB1 => OpCode::GT,
            0xB2 => OpCode::GE,
            0xB3 => OpCode::MIN,
            0xB4 => OpCode::MAX,
            0xB5 => OpCode::WITHIN,
            0xB6 => OpCode::UNKNOWN_B6, // Found in Contract_Assert
            0xB8 => OpCode::UNKNOWN_B8, // Found in Contract_Throw
            0xBB => OpCode::UNKNOWN_BB, // Found in Contract_BigInteger

            // Compound types (0xBE-0xD4)
            0xBE => OpCode::PACKMAP,
            0xBF => OpCode::PACKSTRUCT,
            0xC0 => OpCode::PACKARRAY,
            0xC1 => OpCode::PACK,
            0xC2 => OpCode::UNPACK,
            0xC3 => OpCode::NEWARRAY0,
            0xC4 => OpCode::NEWARRAY,
            0xC5 => OpCode::NEWARRAYT,
            0xC6 => OpCode::NEWSTRUCT0,
            0xC7 => OpCode::NEWSTRUCT,
            0xC8 => OpCode::NEWMAP,
            0xCA => OpCode::APPEND,
            0xCB => OpCode::SETITEM,
            0xCC => OpCode::PICKITEM,
            0xCD => OpCode::REMOVE,
            0xCE => OpCode::CLEARITEMS,
            0xCF => OpCode::POPITEM,
            0xD0 => OpCode::HASKEY,
            0xD1 => OpCode::KEYS,
            0xD2 => OpCode::VALUES,
            0xD3 => OpCode::SLICE,

            // Types (0xD8-0xDB)
            0xD8 => OpCode::ISNULL,
            0xD9 => OpCode::ISTYPE,
            0xDB => OpCode::CONVERT, // Official Neo N3 CONVERT mapping

            // Extensions (0xE0-0xE1)
            0xE0 => OpCode::ABORTMSG,
            0xE1 => OpCode::ASSERTMSG,

            // Missing opcodes found in contract artifacts
            0x07 => OpCode::UNKNOWN_07, // Found in Contract_GoTo
            0x42 => OpCode::UNKNOWN_42, // Found in Contract_Array, Contract_String
            0x44 => OpCode::UNKNOWN_44, // Found in Contract_Switch
            0x94 => OpCode::UNKNOWN_94, // Found in Contract_NULL
            0xB7 => OpCode::UNKNOWN_B7, // Found in Contract_NULL
            0xDA => OpCode::UNKNOWN_DA, // Found in Contract_Array
            0xE4 => OpCode::UNKNOWN_E4, // Found in Contract_Array, Contract_Delegate, Contract_StaticVar, Contract_String, Contract_Types
            0xE6 => OpCode::UNKNOWN_E6, // Found in Contract_NULL
            0xE8 => OpCode::UNKNOWN_E8, // Found in Contract_NULL
            0xE9 => OpCode::UNKNOWN_E9, // Found in Contract_Array
            0xEA => OpCode::UNKNOWN_EA, // Found in Contract_String
            0xEC => OpCode::UNKNOWN_EC, // Found in Contract_Lambda
            0xEF => OpCode::UNKNOWN_EF, // Found in Contract_Array
            0xF0 => OpCode::UNKNOWN_F0, // Found in Contract_Lambda
            0xF2 => OpCode::UNKNOWN_F2, // Found in Contract_String
            0xF7 => OpCode::UNKNOWN_F7, // Found in Contract_String, Contract_Array
            0xFF => OpCode::UNKNOWN_FF, // Found in Contract_Delegate

            _ => OpCode::UNKNOWN(byte),
        }
    }

    /// Get opcode byte value
    pub fn to_byte(self) -> u8 {
        match self {
            // Constants (0x00-0x20)
            OpCode::PUSHINT8 => 0x00,
            OpCode::PUSHINT16 => 0x01,
            OpCode::PUSHINT32 => 0x02,
            OpCode::PUSHINT64 => 0x03,
            OpCode::PUSHINT128 => 0x04,
            OpCode::PUSHINT256 => 0x05,
            OpCode::PUSHT => 0x08,
            OpCode::PUSHF => 0x09,
            OpCode::PUSHA => 0x0A,
            OpCode::PUSHNULL => 0x0B,
            OpCode::PUSHDATA1 => 0x0C,
            OpCode::PUSHDATA2 => 0x0D,
            OpCode::PUSHDATA4 => 0x0E,
            OpCode::PUSHM1 => 0x0F,
            OpCode::PUSH0 => 0x10,
            OpCode::PUSH1 => 0x11,
            OpCode::PUSH2 => 0x12,
            OpCode::PUSH3 => 0x13,
            OpCode::PUSH4 => 0x14,
            OpCode::PUSH5 => 0x15,
            OpCode::PUSH6 => 0x16,
            OpCode::PUSH7 => 0x17,
            OpCode::PUSH8 => 0x18,
            OpCode::PUSH9 => 0x19,
            OpCode::PUSH10 => 0x1A,
            OpCode::PUSH11 => 0x1B,
            OpCode::PUSH12 => 0x1C,
            OpCode::PUSH13 => 0x1D,
            OpCode::PUSH14 => 0x1E,
            OpCode::PUSH15 => 0x1F,
            OpCode::PUSH16 => 0x20,

            // Flow Control (0x21-0x41)
            OpCode::NOP => 0x21,
            OpCode::JMP => 0x22,
            OpCode::JMP_L => 0x23,
            OpCode::JMPIF => 0x24,
            OpCode::JMPIF_L => 0x25,
            OpCode::JMPIFNOT => 0x26,
            OpCode::JMPIFNOT_L => 0x27,
            OpCode::JMPEQ => 0x28,
            OpCode::JMPEQ_L => 0x29,
            OpCode::JMPNE => 0x2A,
            OpCode::JMPNE_L => 0x2B,
            OpCode::JMPGT => 0x2C,
            OpCode::JMPGT_L => 0x2D,
            OpCode::JMPGE => 0x2E,
            OpCode::JMPGE_L => 0x2F,
            OpCode::JMPLT => 0x30,
            OpCode::JMPLT_L => 0x31,
            OpCode::JMPLE => 0x32,
            OpCode::JMPLE_L => 0x33,
            OpCode::CALL => 0x34,
            OpCode::CALL_L => 0x35,
            OpCode::CALLA => 0x36,
            OpCode::CALLT => 0x37,
            OpCode::ABORT => 0x38,
            OpCode::ASSERT => 0x39,
            OpCode::THROW => 0x3A,
            OpCode::TRY => 0x3B,
            OpCode::TRY_L => 0x3C,
            OpCode::ENDTRY => 0x3D,
            OpCode::ENDTRY_L => 0x3E,
            OpCode::ENDFINALLY => 0x3F,
            OpCode::RET => 0x40,
            OpCode::SYSCALL => 0x41,

            OpCode::UNKNOWN(byte) => byte,
            _ => 0x00, // Default fallback for remaining opcodes
        }
    }

    /// Check if opcode is a jump instruction
    pub fn is_jump(&self) -> bool {
        matches!(
            self,
            OpCode::JMP
                | OpCode::JMP_L
                | OpCode::JMPIF
                | OpCode::JMPIF_L
                | OpCode::JMPIFNOT
                | OpCode::JMPIFNOT_L
                | OpCode::JMPEQ
                | OpCode::JMPEQ_L
                | OpCode::JMPNE
                | OpCode::JMPNE_L
                | OpCode::JMPGT
                | OpCode::JMPGT_L
                | OpCode::JMPGE
                | OpCode::JMPGE_L
                | OpCode::JMPLT
                | OpCode::JMPLT_L
                | OpCode::JMPLE
                | OpCode::JMPLE_L
        )
    }

    /// Check if opcode is a call instruction
    pub fn is_call(&self) -> bool {
        matches!(
            self,
            OpCode::CALL | OpCode::CALL_L | OpCode::CALLA | OpCode::CALLT | OpCode::SYSCALL
        )
    }

    /// Check if opcode terminates a basic block
    pub fn is_terminator(&self) -> bool {
        matches!(
            self,
            OpCode::JMP
                | OpCode::JMP_L
                | OpCode::JMPIF
                | OpCode::JMPIF_L
                | OpCode::JMPIFNOT
                | OpCode::JMPIFNOT_L
                | OpCode::JMPEQ
                | OpCode::JMPEQ_L
                | OpCode::JMPNE
                | OpCode::JMPNE_L
                | OpCode::JMPGT
                | OpCode::JMPGT_L
                | OpCode::JMPGE
                | OpCode::JMPGE_L
                | OpCode::JMPLT
                | OpCode::JMPLT_L
                | OpCode::JMPLE
                | OpCode::JMPLE_L
                | OpCode::RET
                | OpCode::ABORT
                | OpCode::THROW
        )
    }

    /// Check if opcode has short and long forms
    pub fn has_long_form(&self) -> bool {
        matches!(
            self,
            OpCode::JMP
                | OpCode::JMPIF
                | OpCode::JMPIFNOT
                | OpCode::JMPEQ
                | OpCode::JMPNE
                | OpCode::JMPGT
                | OpCode::JMPGE
                | OpCode::JMPLT
                | OpCode::JMPLE
                | OpCode::CALL
                | OpCode::TRY
                | OpCode::ENDTRY
        )
    }

    /// Get the long form equivalent of a short form opcode
    pub fn to_long_form(&self) -> OpCode {
        match self {
            OpCode::JMP => OpCode::JMP_L,
            OpCode::JMPIF => OpCode::JMPIF_L,
            OpCode::JMPIFNOT => OpCode::JMPIFNOT_L,
            OpCode::JMPEQ => OpCode::JMPEQ_L,
            OpCode::JMPNE => OpCode::JMPNE_L,
            OpCode::JMPGT => OpCode::JMPGT_L,
            OpCode::JMPGE => OpCode::JMPGE_L,
            OpCode::JMPLT => OpCode::JMPLT_L,
            OpCode::JMPLE => OpCode::JMPLE_L,
            OpCode::CALL => OpCode::CALL_L,
            OpCode::TRY => OpCode::TRY_L,
            OpCode::ENDTRY => OpCode::ENDTRY_L,
            _ => *self,
        }
    }

    /// Check if this is a long form opcode
    pub fn is_long_form(&self) -> bool {
        matches!(
            self,
            OpCode::JMP_L
                | OpCode::JMPIF_L
                | OpCode::JMPIFNOT_L
                | OpCode::JMPEQ_L
                | OpCode::JMPNE_L
                | OpCode::JMPGT_L
                | OpCode::JMPGE_L
                | OpCode::JMPLT_L
                | OpCode::JMPLE_L
                | OpCode::CALL_L
                | OpCode::TRY_L
                | OpCode::ENDTRY_L
        )
    }
}

/// Instruction operand types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Operand {
    /// Immediate integer value (various sizes)
    Integer(i64),
    /// Big integer value (128-bit or 256-bit)
    BigInteger(Vec<u8>),
    /// Immediate bytes
    Bytes(Vec<u8>),
    /// Jump target offset (legacy name for compatibility)
    JumpTarget(i32),
    /// Jump target offset (short form - 1 byte signed)
    JumpTarget8(i8),
    /// Jump target offset (long form - 4 bytes signed)
    JumpTarget32(i32),
    /// Local/argument slot index
    SlotIndex(u8),
    /// Syscall hash/identifier
    SyscallHash(u32),
    /// Type conversion target
    StackItemType(StackItemType),
    /// Try-catch block info
    TryBlock {
        catch_offset: u32,
        finally_offset: Option<u32>,
    },
    /// Slot initialization info (static slots, local slots)
    SlotInit { static_slots: u8, local_slots: u8 },
    /// Method token for CALLA
    MethodToken(u16),
    /// Call token for CALLT  
    CallToken(u16),
    /// Buffer size
    BufferSize(u16),
    /// Array/structure count
    Count(u8),
    /// String message for ABORTMSG/ASSERTMSG
    Message(String),
}

/// Neo N3 stack item types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StackItemType {
    Any,
    Boolean,
    Integer,
    ByteString,
    Buffer,
    Array,
    Struct,
    Map,
    InteropInterface,
    Pointer,
}

/// Basic block identifier
pub type BlockId = u32;

/// Type variable for type inference
pub type TypeVar = u32;

/// Contract hash type (160-bit hash)
pub type Hash160 = [u8; 20];

/// Block hash type (256-bit hash)
pub type Hash256 = [u8; 32];

/// Contract identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContractId {
    Hash(Hash160),
    Name(String),
}

/// Variable reference in IR
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub id: u32,
    pub var_type: VariableType,
}

/// Variable type classification
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VariableType {
    Local,
    Parameter,
    Static,
    Temporary,
}

/// Literal values in expressions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Boolean(bool),
    Integer(i64),
    BigInteger(Vec<u8>),
    String(String),
    ByteArray(Vec<u8>),
    Hash160(Hash160),
    Hash256(Hash256),
    Null,
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    // Alternative names for compatibility
    Subtract,
    Multiply,
    Divide,
    And,
    Or,
    Xor,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    BoolAnd,
    BoolOr,
    LeftShift,
    RightShift,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnaryOperator {
    Not,
    Negate,
    BoolNot,
    Abs,
    Sign,
    Sqrt,
    BitwiseNot,
}

/// Storage operation types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StorageOp {
    Get,
    Put,
    Delete,
    Find,
}

impl std::fmt::Display for OpCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpCode::UNKNOWN(byte) => write!(f, "UNKNOWN(0x{:02X})", byte),
            _ => write!(f, "{:?}", self),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_conversion() {
        assert_eq!(OpCode::from_byte(0x00), OpCode::PUSHINT8);
        assert_eq!(OpCode::PUSHINT8.to_byte(), 0x00);
        assert_eq!(OpCode::from_byte(0xFF), OpCode::UNKNOWN(0xFF));
    }

    #[test]
    fn test_opcode_properties() {
        assert!(OpCode::JMP.is_jump());
        assert!(OpCode::CALL.is_call());
        assert!(OpCode::RET.is_terminator());
        assert!(!OpCode::ADD.is_jump());
    }
}

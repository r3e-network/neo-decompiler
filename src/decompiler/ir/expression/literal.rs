use std::fmt;

/// Literal values that can appear in expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// Integer literal.
    Int(i64),
    /// Big integer literal (for PUSHINT128/256).
    BigInt(String),
    /// Boolean literal.
    Bool(bool),
    /// String literal.
    String(String),
    /// Byte array literal.
    Bytes(Vec<u8>),
    /// Null value.
    Null,
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::Int(n) => write!(f, "{}", n),
            Literal::BigInt(s) => write!(f, "BigInteger(\"{}\")", s),
            Literal::Bool(b) => write!(f, "{}", b),
            Literal::String(s) => write!(f, "\"{}\"", s.escape_default()),
            Literal::Bytes(b) => write!(f, "0x{}", hex::encode(b)),
            Literal::Null => write!(f, "null"),
        }
    }
}

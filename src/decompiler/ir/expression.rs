//! Expression IR nodes for decompiled code.

use std::fmt;

/// Literal values that can appear in expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// Integer literal (supports arbitrary precision via string).
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

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    // Bitwise
    And,
    Or,
    Xor,
    Shl,
    Shr,
    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // Logical
    LogicalAnd,
    LogicalOr,
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Pow => "**",
            BinOp::And => "&",
            BinOp::Or => "|",
            BinOp::Xor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::LogicalAnd => "&&",
            BinOp::LogicalOr => "||",
        };
        write!(f, "{}", op)
    }
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Arithmetic negation.
    Neg,
    /// Bitwise NOT.
    Not,
    /// Logical NOT.
    LogicalNot,
    /// Increment.
    Inc,
    /// Decrement.
    Dec,
    /// Absolute value.
    Abs,
    /// Sign function.
    Sign,
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = match self {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "~",
            UnaryOp::LogicalNot => "!",
            UnaryOp::Inc => "++",
            UnaryOp::Dec => "--",
            UnaryOp::Abs => "abs",
            UnaryOp::Sign => "sign",
        };
        write!(f, "{}", op)
    }
}

/// Expression nodes in the IR.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Literal constant value.
    Literal(Literal),
    /// Variable reference (local, arg, or slot).
    Variable(String),
    /// Binary operation.
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// Unary operation.
    Unary { op: UnaryOp, operand: Box<Expr> },
    /// Function or syscall invocation.
    Call { name: String, args: Vec<Expr> },
    /// Array/map index access.
    Index { base: Box<Expr>, index: Box<Expr> },
    /// Field/member access.
    Member { base: Box<Expr>, name: String },
    /// Type cast.
    Cast {
        expr: Box<Expr>,
        target_type: String,
    },
    /// Array literal.
    Array(Vec<Expr>),
    /// Map literal (key-value pairs).
    Map(Vec<(Expr, Expr)>),
    /// Ternary conditional expression.
    Ternary {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    /// Stack temporary (for unlifted operations).
    StackTemp(usize),
}

impl Expr {
    /// Create a new literal integer expression.
    pub fn int(n: i64) -> Self {
        Expr::Literal(Literal::Int(n))
    }

    /// Create a new variable reference.
    pub fn var(name: impl Into<String>) -> Self {
        Expr::Variable(name.into())
    }

    /// Create a binary expression.
    pub fn binary(op: BinOp, left: Expr, right: Expr) -> Self {
        Expr::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Create a unary expression.
    pub fn unary(op: UnaryOp, operand: Expr) -> Self {
        Expr::Unary {
            op,
            operand: Box::new(operand),
        }
    }

    /// Create a function call expression.
    pub fn call(name: impl Into<String>, args: Vec<Expr>) -> Self {
        Expr::Call {
            name: name.into(),
            args,
        }
    }

    /// Create an index expression.
    pub fn index(base: Expr, index: Expr) -> Self {
        Expr::Index {
            base: Box::new(base),
            index: Box::new(index),
        }
    }
}

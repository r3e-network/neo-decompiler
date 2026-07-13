use super::literal::Literal;
use super::operators::{BinOp, UnaryOp};
use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::ir::SemanticCallTarget;

/// Expression nodes in the IR.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A VM stack value whose producer could not be recovered.
    Unknown,
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
    Call {
        target: SemanticCallTarget,
        args: Vec<Expr>,
    },
    /// Array/map index access.
    Index { base: Box<Expr>, index: Box<Expr> },
    /// Field/member access.
    Member { base: Box<Expr>, name: String },
    /// Type cast.
    Cast {
        expr: Box<Expr>,
        target_type: String,
    },
    /// Neo VM conversion retaining its StackItemType operand.
    Convert { value: Box<Expr>, target: ValueType },
    /// Neo VM runtime type check retaining its StackItemType operand.
    IsType { value: Box<Expr>, target: ValueType },
    /// Sized array construction with an optional typed element tag.
    NewArray {
        length: Box<Expr>,
        element_type: Option<ValueType>,
    },
    /// Array literal.
    Array(Vec<Expr>),
    /// Struct literal.
    Struct(Vec<Expr>),
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
    #[must_use]
    pub fn int(n: i64) -> Self {
        Expr::Literal(Literal::Int(n))
    }

    /// Create a new variable reference.
    #[must_use]
    pub fn var(name: impl Into<String>) -> Self {
        Expr::Variable(name.into())
    }

    /// Create a binary expression.
    #[must_use]
    pub fn binary(op: BinOp, left: Expr, right: Expr) -> Self {
        Expr::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Create a unary expression.
    #[must_use]
    pub fn unary(op: UnaryOp, operand: Expr) -> Self {
        Expr::Unary {
            op,
            operand: Box::new(operand),
        }
    }

    /// Create a call whose semantic target is known.
    #[must_use]
    pub fn call(target: SemanticCallTarget, args: Vec<Expr>) -> Self {
        Expr::Call { target, args }
    }

    /// Create an unresolved call for analysis-only or hand-built IR.
    #[must_use]
    pub fn unresolved_call(display_name: impl Into<String>, args: Vec<Expr>) -> Self {
        Expr::Call {
            target: SemanticCallTarget::Unresolved {
                display_name: display_name.into(),
            },
            args,
        }
    }

    /// Create an index expression.
    #[must_use]
    pub fn index(base: Expr, index: Expr) -> Self {
        Expr::Index {
            base: Box::new(base),
            index: Box::new(index),
        }
    }
}

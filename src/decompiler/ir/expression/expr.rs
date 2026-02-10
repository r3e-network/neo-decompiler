use super::literal::Literal;
use super::operators::{BinOp, UnaryOp};

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

    /// Create a function call expression.
    #[must_use]
    pub fn call(name: impl Into<String>, args: Vec<Expr>) -> Self {
        Expr::Call {
            name: name.into(),
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

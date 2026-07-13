//! Statement IR nodes for decompiled code.

use super::control_flow::ControlFlow;
use super::expression::Expr;

/// Deterministic label assigned to a CFG basic block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockLabel(pub usize);

/// Statement nodes in the IR.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// Variable assignment.
    Assign { target: String, value: Expr },
    /// Return statement with optional value.
    Return(Option<Expr>),
    /// Catchable VM throw with an optional payload.
    Throw(Option<Expr>),
    /// Uncatchable VM abort with an optional diagnostic payload.
    Abort(Option<Expr>),
    /// Runtime assertion with an optional failure message.
    Assert {
        /// Value tested by the assertion.
        condition: Expr,
        /// Optional diagnostic value produced when the assertion fails.
        message: Option<Expr>,
    },
    /// Expression evaluated for side effects.
    ExprStmt(Expr),
    /// Inline comment.
    Comment(String),
    /// Exit the nearest enclosing loop or switch.
    Break,
    /// Continue the nearest enclosing loop.
    Continue,
    /// Label an irreducible basic block.
    Label(BlockLabel),
    /// Transfer to a labeled irreducible basic block.
    Goto(BlockLabel),
    /// Control flow construct.
    ControlFlow(Box<ControlFlow>),
}

impl Stmt {
    /// Create an assignment statement.
    pub fn assign(target: impl Into<String>, value: Expr) -> Self {
        Stmt::Assign {
            target: target.into(),
            value,
        }
    }

    /// Create a return statement with a value.
    #[must_use]
    pub fn ret(value: Expr) -> Self {
        Stmt::Return(Some(value))
    }

    /// Create a return statement without a value.
    #[must_use]
    pub fn ret_void() -> Self {
        Stmt::Return(None)
    }

    /// Create a catchable throw statement.
    #[must_use]
    pub fn throw(value: Option<Expr>) -> Self {
        Stmt::Throw(value)
    }

    /// Create an uncatchable abort statement.
    #[must_use]
    pub fn abort(message: Option<Expr>) -> Self {
        Stmt::Abort(message)
    }

    /// Create an assertion statement.
    #[must_use]
    pub fn assert(condition: Expr, message: Option<Expr>) -> Self {
        Stmt::Assert { condition, message }
    }

    /// Create an expression statement.
    #[must_use]
    pub fn expr(e: Expr) -> Self {
        Stmt::ExprStmt(e)
    }

    /// Create a comment statement.
    pub fn comment(text: impl Into<String>) -> Self {
        Stmt::Comment(text.into())
    }
}

/// A block of statements.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

impl Block {
    /// Create a new empty block.
    #[must_use]
    pub fn new() -> Self {
        Self { stmts: Vec::new() }
    }

    /// Create a block with statements.
    #[must_use]
    pub fn with_stmts(stmts: Vec<Stmt>) -> Self {
        Self { stmts }
    }

    /// Add a statement to the block.
    pub fn push(&mut self, stmt: Stmt) {
        self.stmts.push(stmt);
    }

    /// Check if the block is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stmts.is_empty()
    }

    /// Get the number of statements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.stmts.len()
    }
}

impl From<Vec<Stmt>> for Block {
    fn from(stmts: Vec<Stmt>) -> Self {
        Self { stmts }
    }
}

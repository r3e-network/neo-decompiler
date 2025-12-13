//! Statement IR nodes for decompiled code.

use super::control_flow::ControlFlow;
use super::expression::Expr;

/// Statement nodes in the IR.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// Variable assignment.
    Assign { target: String, value: Expr },
    /// Return statement with optional value.
    Return(Option<Expr>),
    /// Expression evaluated for side effects.
    ExprStmt(Expr),
    /// Inline comment.
    Comment(String),
    /// Control flow construct.
    ControlFlow(Box<ControlFlow>),
    /// Variable declaration with optional initialization.
    VarDecl {
        name: String,
        var_type: Option<String>,
        init: Option<Expr>,
    },
    /// Throw/abort statement.
    Throw(Option<Expr>),
    /// Break statement (for loops).
    Break,
    /// Continue statement (for loops).
    Continue,
    /// Unreachable placeholder for unlifted opcodes.
    Unlifted {
        offset: usize,
        opcode: String,
        comment: String,
    },
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
    pub fn ret(value: Expr) -> Self {
        Stmt::Return(Some(value))
    }

    /// Create a return statement without a value.
    pub fn ret_void() -> Self {
        Stmt::Return(None)
    }

    /// Create an expression statement.
    pub fn expr(e: Expr) -> Self {
        Stmt::ExprStmt(e)
    }

    /// Create a comment statement.
    pub fn comment(text: impl Into<String>) -> Self {
        Stmt::Comment(text.into())
    }

    /// Create an unlifted opcode placeholder.
    pub fn unlifted(offset: usize, opcode: impl Into<String>, comment: impl Into<String>) -> Self {
        Stmt::Unlifted {
            offset,
            opcode: opcode.into(),
            comment: comment.into(),
        }
    }
}

/// A block of statements.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

impl Block {
    /// Create a new empty block.
    pub fn new() -> Self {
        Self { stmts: Vec::new() }
    }

    /// Create a block with statements.
    pub fn with_stmts(stmts: Vec<Stmt>) -> Self {
        Self { stmts }
    }

    /// Add a statement to the block.
    pub fn push(&mut self, stmt: Stmt) {
        self.stmts.push(stmt);
    }

    /// Check if the block is empty.
    pub fn is_empty(&self) -> bool {
        self.stmts.is_empty()
    }

    /// Get the number of statements.
    pub fn len(&self) -> usize {
        self.stmts.len()
    }
}

impl From<Vec<Stmt>> for Block {
    fn from(stmts: Vec<Stmt>) -> Self {
        Self { stmts }
    }
}

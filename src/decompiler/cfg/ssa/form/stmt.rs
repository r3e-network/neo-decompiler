//! SSA statement vocabulary and constructors.

use std::fmt;

use crate::decompiler::ir::Stmt;

use super::super::variable::{PhiNode, SsaVariable};
use super::expr::SsaExpr;

/// A statement in SSA form.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SsaStmt {
    /// Variable assignment with SSA target.
    Assign {
        /// The SSA variable being defined.
        target: SsaVariable,
        /// The value being assigned (in SSA expression form).
        value: SsaExpr,
    },

    /// Expression evaluated for side effects, such as a void call.
    Expr(SsaExpr),

    /// Method return with the evaluation stack's top value, if present.
    Return(Option<SsaExpr>),

    /// Catchable VM throw with an optional payload.
    Throw(Option<SsaExpr>),

    /// Uncatchable VM abort with an optional diagnostic payload.
    Abort(Option<SsaExpr>),

    /// Runtime assertion with an optional failure message.
    Assert {
        /// Value tested by the assertion.
        condition: SsaExpr,
        /// Optional diagnostic value produced when the assertion fails.
        message: Option<SsaExpr>,
    },

    /// φ node (internal representation, typically transformed before output).
    Phi(PhiNode),

    /// Other statements that don't define SSA variables.
    Other(Stmt),
}

impl SsaStmt {
    /// Create an assignment statement.
    #[must_use]
    pub fn assign(target: SsaVariable, value: SsaExpr) -> Self {
        Self::Assign { target, value }
    }

    /// Create an expression statement.
    #[must_use]
    pub fn expr(value: SsaExpr) -> Self {
        Self::Expr(value)
    }

    /// Create a return statement.
    #[must_use]
    pub fn ret(value: Option<SsaExpr>) -> Self {
        Self::Return(value)
    }

    /// Create a catchable throw statement.
    #[must_use]
    pub fn throw(value: Option<SsaExpr>) -> Self {
        Self::Throw(value)
    }

    /// Create an uncatchable abort statement.
    #[must_use]
    pub fn abort(message: Option<SsaExpr>) -> Self {
        Self::Abort(message)
    }

    /// Create an assertion statement.
    #[must_use]
    pub fn assert(condition: SsaExpr, message: Option<SsaExpr>) -> Self {
        Self::Assert { condition, message }
    }

    /// Create a φ node statement.
    #[must_use]
    pub const fn phi(phi: PhiNode) -> Self {
        Self::Phi(phi)
    }

    /// Wrap a regular statement.
    #[must_use]
    pub const fn other(stmt: Stmt) -> Self {
        Self::Other(stmt)
    }
}

impl fmt::Display for SsaStmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Assign { target, value } => write!(f, "{} = {};", target, value),
            Self::Expr(value) => write!(f, "{value};"),
            Self::Return(Some(value)) => write!(f, "return {value};"),
            Self::Return(None) => write!(f, "return;"),
            Self::Throw(Some(value)) => write!(f, "throw({value});"),
            Self::Throw(None) => write!(f, "throw();"),
            Self::Abort(Some(message)) => write!(f, "abort({message});"),
            Self::Abort(None) => write!(f, "abort();"),
            Self::Assert {
                condition,
                message: Some(message),
            } => write!(f, "assert({condition}, {message});"),
            Self::Assert {
                condition,
                message: None,
            } => write!(f, "assert({condition});"),
            Self::Phi(phi) => write!(f, "{}", phi), // φ nodes have their own Display
            Self::Other(stmt) => write!(f, "{:?}", stmt), // Use debug for other statements
        }
    }
}

//! SSA expression vocabulary and constructors.

use std::fmt;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::ir::{BinOp, Literal, SemanticCallTarget, UnaryOp};

use super::super::variable::SsaVariable;

/// An expression in SSA form.
///
/// SSA expressions reference `SsaVariable` instead of raw strings,
/// ensuring version tracking through the SSA transformation.
#[derive(Debug, Clone, PartialEq)]
pub enum SsaExpr {
    /// SSA variable reference.
    Variable(SsaVariable),

    /// Literal constant value.
    Literal(Literal),

    /// Binary operation.
    Binary {
        /// The binary operator.
        op: BinOp,
        /// Left-hand operand.
        left: Box<SsaExpr>,
        /// Right-hand operand.
        right: Box<SsaExpr>,
    },

    /// Unary operation.
    Unary {
        /// The unary operator.
        op: UnaryOp,
        /// The operand.
        operand: Box<SsaExpr>,
    },

    /// Function or syscall invocation.
    Call {
        /// Semantic call identity and display metadata.
        target: SemanticCallTarget,
        /// Call arguments.
        args: Vec<SsaExpr>,
    },

    /// Array/map index access.
    Index {
        /// Base expression being indexed.
        base: Box<SsaExpr>,
        /// Index expression.
        index: Box<SsaExpr>,
    },

    /// Field/member access.
    Member {
        /// Base expression.
        base: Box<SsaExpr>,
        /// Field name.
        name: String,
    },

    /// Type cast.
    Cast {
        /// Expression being cast.
        expr: Box<SsaExpr>,
        /// Target type name.
        target_type: String,
    },

    /// Neo VM conversion retaining its StackItemType operand.
    Convert {
        /// Value to convert.
        value: Box<SsaExpr>,
        /// Requested VM target type.
        target: ValueType,
    },

    /// Neo VM runtime type check retaining its StackItemType operand.
    IsType {
        /// Value to inspect.
        value: Box<SsaExpr>,
        /// Requested VM target type.
        target: ValueType,
    },

    /// Sized array construction with an optional typed element tag.
    NewArray {
        /// Requested array length.
        length: Box<SsaExpr>,
        /// Element type carried by NEWARRAY_T, if present.
        element_type: Option<ValueType>,
    },

    /// Array literal.
    Array(Vec<SsaExpr>),

    /// Struct literal.
    Struct(Vec<SsaExpr>),

    /// Map literal (key-value pairs).
    Map(Vec<(SsaExpr, SsaExpr)>),

    /// Ternary conditional expression.
    Ternary {
        /// Condition expression.
        condition: Box<SsaExpr>,
        /// Value when condition is true.
        then_expr: Box<SsaExpr>,
        /// Value when condition is false.
        else_expr: Box<SsaExpr>,
    },
}

impl SsaExpr {
    /// Create a variable reference.
    #[must_use]
    pub fn var(var: SsaVariable) -> Self {
        Self::Variable(var)
    }

    /// Create a literal expression.
    #[must_use]
    pub const fn lit(literal: Literal) -> Self {
        Self::Literal(literal)
    }

    /// Create a binary expression.
    #[must_use]
    pub fn binary(op: BinOp, left: SsaExpr, right: SsaExpr) -> Self {
        Self::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Create a unary expression.
    #[must_use]
    pub fn unary(op: UnaryOp, operand: SsaExpr) -> Self {
        Self::Unary {
            op,
            operand: Box::new(operand),
        }
    }

    /// Create a call whose semantic target is known.
    #[must_use]
    pub fn call(target: SemanticCallTarget, args: Vec<SsaExpr>) -> Self {
        Self::Call { target, args }
    }

    /// Create an unresolved call for analysis-only or hand-built SSA.
    #[must_use]
    pub fn unresolved_call(display_name: impl Into<String>, args: Vec<SsaExpr>) -> Self {
        Self::Call {
            target: SemanticCallTarget::Unresolved {
                display_name: display_name.into(),
            },
            args,
        }
    }
}

impl fmt::Display for SsaExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Variable(var) => write!(f, "{}", var),
            Self::Literal(lit) => write!(f, "{}", lit),
            Self::Binary { op, left, right } => write!(f, "({} {} {})", left, op, right),
            Self::Unary { op, operand } => write!(f, "{}({})", op, operand),
            Self::Call { target, args } => {
                write!(f, "{}(", target.display_name())?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            Self::Index { base, index } => write!(f, "{}[{}]", base, index),
            Self::Member { base, name } => write!(f, "{}.{}", base, name),
            Self::Cast { expr, target_type } => write!(f, "{} as {}", expr, target_type),
            Self::Convert { value, target } => write!(f, "convert({value}, {target})"),
            Self::IsType { value, target } => write!(f, "is_type({value}, {target})"),
            Self::NewArray {
                length,
                element_type,
            } => match element_type {
                Some(element_type) => write!(f, "new_array({length}, {element_type})"),
                None => write!(f, "new_array({length})"),
            },
            Self::Array(elements) => {
                write!(f, "[")?;
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", elem)?;
                }
                write!(f, "]")
            }
            Self::Struct(elements) => {
                write!(f, "struct[")?;
                for (i, element) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{element}")?;
                }
                write!(f, "]")
            }
            Self::Map(pairs) => {
                write!(f, "{{")?;
                for (i, (key, value)) in pairs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", key, value)?;
                }
                write!(f, "}}")
            }
            Self::Ternary {
                condition,
                then_expr,
                else_expr,
            } => write!(f, "{} ? {} : {}", condition, then_expr, else_expr),
        }
    }
}

use std::collections::BTreeSet;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::csharp::helpers::format_vm_truthiness;
use crate::decompiler::ir::{BinOp, Expr, Literal, UnaryOp};
use crate::instruction::OpCode;

use super::expr_calls::render_call;
pub(super) use super::expr_context::ExprContext;
pub(in crate::decompiler::csharp::render) use super::expr_low_level::{
    default_tagged_opcode_helper_name, tagged_opcode_helper_key,
};
use super::expr_low_level::{
    render_low_level_boolean_binary_opcode, render_low_level_opcode, render_tagged_type_opcode,
    render_tagged_type_opcode_source,
};
pub(super) use super::expr_values::{escape_csharp_string, int_cast, render_expr_list};
use super::expr_values::{render_literal, render_new_array};

pub(super) const PREC_ASSIGNMENT: u8 = 1;
pub(super) const PREC_TERNARY: u8 = 2;
const PREC_BIT_OR: u8 = 5;
const PREC_BIT_XOR: u8 = 6;
const PREC_BIT_AND: u8 = 7;
pub(super) const PREC_EQUALITY: u8 = 8;
pub(super) const PREC_RELATIONAL: u8 = 9;
const PREC_SHIFT: u8 = 10;
const PREC_ADDITIVE: u8 = 11;
const PREC_MULTIPLICATIVE: u8 = 12;
pub(super) const PREC_UNARY: u8 = 13;
pub(super) const PREC_PRIMARY: u8 = 14;

#[derive(Debug)]
pub(super) struct RenderedExpr {
    pub(super) source: String,
    pub(super) precedence: u8,
}

impl RenderedExpr {
    pub(super) fn new(source: impl Into<String>, precedence: u8) -> Self {
        Self {
            source: source.into(),
            precedence,
        }
    }

    pub(super) fn in_context(self, parent_precedence: u8) -> String {
        if self.precedence < parent_precedence {
            format!("({})", self.source)
        } else {
            self.source
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn render_expr(expression: &Expr, context: &ExprContext) -> String {
    render_expr_prec(expression, 0, context, &mut BTreeSet::new())
}

pub(super) fn render_vm_condition(expression: &Expr, context: &ExprContext) -> String {
    let mut expanding = BTreeSet::new();
    if matches!(
        expression,
        Expr::Binary {
            op: BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::LogicalAnd
                | BinOp::LogicalOr,
            ..
        } | Expr::Unary {
            op: UnaryOp::LogicalNot,
            ..
        } | Expr::IsType { .. }
    ) {
        return render_expr_prec(expression, 0, context, &mut expanding);
    }
    match context.value_type(expression) {
        ValueType::Boolean => render_expr_prec(expression, 0, context, &mut expanding),
        ValueType::Integer => format!(
            "{} != 0",
            render_expr_prec(expression, PREC_EQUALITY + 1, context, &mut expanding)
        ),
        ValueType::Null => "false".to_string(),
        _ => format_vm_truthiness(&render_expr_prec(expression, 0, context, &mut expanding)),
    }
}

pub(super) fn render_expr_prec(
    expression: &Expr,
    parent_precedence: u8,
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> String {
    render_expr_node(expression, context, expanding).in_context(parent_precedence)
}

fn render_expr_node(
    expression: &Expr,
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    match expression {
        Expr::Unknown => RenderedExpr::new("(dynamic)null", PREC_UNARY),
        Expr::Literal(literal) => RenderedExpr::new(
            render_literal(literal),
            if matches!(literal, Literal::Bytes(_))
                || matches!(literal, Literal::Int(value) if *value < 0)
            {
                PREC_UNARY
            } else {
                PREC_PRIMARY
            },
        ),
        Expr::Variable(name) => {
            if expanding.insert(name.clone()) {
                if let Some(value) = context.inline_values.get(name) {
                    let rendered = render_expr_node(value, context, expanding);
                    expanding.remove(name);
                    return rendered;
                }
                expanding.remove(name);
            }
            RenderedExpr::new(
                context
                    .emitted_names
                    .get(name)
                    .map_or(name.as_str(), String::as_str),
                PREC_PRIMARY,
            )
        }
        Expr::Binary { op, left, right } => render_binary(*op, left, right, context, expanding),
        Expr::Unary { op, operand } => render_unary(*op, operand, context, expanding),
        Expr::Call { target, args } => render_call(target, args, context, expanding),
        Expr::Index { base, index } => RenderedExpr::new(
            format!(
                "{}[{}]",
                render_expr_prec(base, PREC_PRIMARY, context, expanding),
                render_expr_prec(index, 0, context, expanding)
            ),
            PREC_PRIMARY,
        ),
        Expr::Member { base, name } => RenderedExpr::new(
            format!(
                "{}.{}",
                render_expr_prec(base, PREC_PRIMARY, context, expanding),
                name
            ),
            PREC_PRIMARY,
        ),
        Expr::Cast { expr, target_type } => RenderedExpr::new(
            format!(
                "({target_type})({})",
                render_expr_prec(expr, 0, context, expanding)
            ),
            PREC_UNARY,
        ),
        Expr::Convert { value, target } => {
            render_tagged_type_opcode(OpCode::Convert, *target, value, context, expanding)
        }
        Expr::IsType { value, target } => {
            render_tagged_type_opcode(OpCode::Istype, *target, value, context, expanding)
        }
        Expr::NewArray {
            length,
            element_type,
        } => RenderedExpr::new(
            render_new_array(length, *element_type, context, expanding),
            PREC_PRIMARY,
        ),
        Expr::Array(elements) => RenderedExpr::new(
            format!(
                "new {} {{ {} }}",
                context.exact_csharp_type(expression).unwrap_or("object[]"),
                render_expr_list(elements, context, expanding)
            ),
            PREC_PRIMARY,
        ),
        Expr::Struct(elements) => {
            let array = format!(
                "new object[] {{ {} }}",
                render_expr_list(elements, context, expanding)
            );
            render_tagged_type_opcode_source(OpCode::Convert, ValueType::Struct, &array, context)
        }
        Expr::Map(pairs) => {
            if pairs.is_empty() {
                return RenderedExpr::new("new Map<object, object>()", PREC_PRIMARY);
            }
            let entries = pairs
                .iter()
                .map(|(key, value)| {
                    format!(
                        "[{}] = {}",
                        render_expr_prec(key, 0, context, expanding),
                        render_expr_prec(value, 0, context, expanding)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            RenderedExpr::new(
                format!("new Map<object, object> {{ {entries} }}"),
                PREC_PRIMARY,
            )
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => RenderedExpr::new(
            format!(
                "{} ? {} : {}",
                render_expr_prec(condition, PREC_TERNARY + 1, context, expanding),
                render_expr_prec(then_expr, PREC_TERNARY + 1, context, expanding),
                render_expr_prec(else_expr, PREC_TERNARY + 1, context, expanding)
            ),
            PREC_TERNARY,
        ),
        Expr::StackTemp(index) => RenderedExpr::new(format!("_tmp{index}"), PREC_PRIMARY),
    }
}

fn render_binary(
    operator: BinOp,
    left: &Expr,
    right: &Expr,
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    if let Some(opcode) = low_level_binary_opcode(
        operator,
        context.value_type(left),
        context.value_type(right),
    ) {
        if matches!(
            operator,
            BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::LogicalAnd
                | BinOp::LogicalOr
        ) {
            return render_low_level_boolean_binary_opcode(opcode, left, right, context, expanding);
        }
        return render_low_level_opcode(opcode, &[left.clone(), right.clone()], context, expanding);
    }
    if operator == BinOp::Pow {
        return RenderedExpr::new(
            format!(
                "BigInteger.Pow({}, {})",
                render_expr_prec(left, 0, context, expanding),
                int_cast(right, context, expanding)
            ),
            PREC_PRIMARY,
        );
    }
    let (spelling, precedence) = binary_spelling(operator);
    let right = if matches!(operator, BinOp::Shl | BinOp::Shr) {
        int_cast(right, context, expanding)
    } else {
        render_expr_prec(right, precedence + 1, context, expanding)
    };
    RenderedExpr::new(
        format!(
            "{} {spelling} {}",
            render_expr_prec(left, precedence, context, expanding),
            right
        ),
        precedence,
    )
}

pub(super) fn low_level_binary_opcode(
    operator: BinOp,
    left_type: ValueType,
    right_type: ValueType,
) -> Option<OpCode> {
    match operator {
        BinOp::Add
        | BinOp::Sub
        | BinOp::Mul
        | BinOp::Div
        | BinOp::Mod
        | BinOp::Pow
        | BinOp::And
        | BinOp::Or
        | BinOp::Xor
        | BinOp::Shl
        | BinOp::Shr
        | BinOp::Lt
        | BinOp::Le
        | BinOp::Gt
        | BinOp::Ge
            if [left_type, right_type].into_iter().any(|value_type| {
                !matches!(value_type, ValueType::Integer | ValueType::Unknown)
            }) =>
        {
            Some(match operator {
                BinOp::Add => OpCode::Add,
                BinOp::Sub => OpCode::Sub,
                BinOp::Mul => OpCode::Mul,
                BinOp::Div => OpCode::Div,
                BinOp::Mod => OpCode::Mod,
                BinOp::Pow => OpCode::Pow,
                BinOp::And => OpCode::And,
                BinOp::Or => OpCode::Or,
                BinOp::Xor => OpCode::Xor,
                BinOp::Shl => OpCode::Shl,
                BinOp::Shr => OpCode::Shr,
                BinOp::Lt => OpCode::Lt,
                BinOp::Le => OpCode::Le,
                BinOp::Gt => OpCode::Gt,
                BinOp::Ge => OpCode::Ge,
                _ => unreachable!(),
            })
        }
        BinOp::Eq | BinOp::Ne
            if !matches!(
                (left_type, right_type),
                (ValueType::Integer, ValueType::Integer) | (ValueType::Boolean, ValueType::Boolean)
            ) =>
        {
            Some(if operator == BinOp::Eq {
                OpCode::Equal
            } else {
                OpCode::Notequal
            })
        }
        BinOp::LogicalAnd | BinOp::LogicalOr
            if !matches!(
                (left_type, right_type),
                (ValueType::Boolean, ValueType::Boolean)
            ) =>
        {
            Some(if operator == BinOp::LogicalAnd {
                OpCode::Booland
            } else {
                OpCode::Boolor
            })
        }
        _ => None,
    }
}

fn binary_spelling(operator: BinOp) -> (&'static str, u8) {
    match operator {
        BinOp::Add => ("+", PREC_ADDITIVE),
        BinOp::Sub => ("-", PREC_ADDITIVE),
        BinOp::Mul => ("*", PREC_MULTIPLICATIVE),
        BinOp::Div => ("/", PREC_MULTIPLICATIVE),
        BinOp::Mod => ("%", PREC_MULTIPLICATIVE),
        BinOp::Pow => unreachable!("power renders as BigInteger.Pow"),
        BinOp::And => ("&", PREC_BIT_AND),
        BinOp::Or => ("|", PREC_BIT_OR),
        BinOp::Xor => ("^", PREC_BIT_XOR),
        BinOp::Shl => ("<<", PREC_SHIFT),
        BinOp::Shr => (">>", PREC_SHIFT),
        BinOp::Eq => ("==", PREC_EQUALITY),
        BinOp::Ne => ("!=", PREC_EQUALITY),
        BinOp::Lt => ("<", PREC_RELATIONAL),
        BinOp::Le => ("<=", PREC_RELATIONAL),
        BinOp::Gt => (">", PREC_RELATIONAL),
        BinOp::Ge => (">=", PREC_RELATIONAL),
        BinOp::LogicalAnd => ("&", PREC_BIT_AND),
        BinOp::LogicalOr => ("|", PREC_BIT_OR),
    }
}

fn render_unary(
    operator: UnaryOp,
    operand: &Expr,
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    match operator {
        UnaryOp::LogicalNot => match context.value_type(operand) {
            ValueType::Boolean => RenderedExpr::new(
                format!(
                    "!{}",
                    render_expr_prec(operand, PREC_UNARY + 1, context, expanding)
                ),
                PREC_UNARY,
            ),
            ValueType::Integer => RenderedExpr::new(
                format!(
                    "(BigInteger)(dynamic)({}) == 0",
                    render_expr_prec(operand, 0, context, expanding)
                ),
                PREC_EQUALITY,
            ),
            ValueType::Null => RenderedExpr::new("true".to_string(), PREC_PRIMARY),
            _ => RenderedExpr::new(
                format!(
                    "!{}",
                    format_vm_truthiness(&render_expr_prec(operand, 0, context, expanding))
                ),
                PREC_UNARY,
            ),
        },
        UnaryOp::Neg | UnaryOp::Not => {
            let spelling = match operator {
                UnaryOp::Neg => "-",
                UnaryOp::Not => "~",
                _ => unreachable!(),
            };
            RenderedExpr::new(
                format!(
                    "{spelling}{}",
                    render_expr_prec(operand, PREC_UNARY + 1, context, expanding)
                ),
                PREC_UNARY,
            )
        }
        UnaryOp::Inc | UnaryOp::Dec => {
            let spelling = if operator == UnaryOp::Inc { "+" } else { "-" };
            RenderedExpr::new(
                format!(
                    "{} {spelling} 1",
                    render_expr_prec(operand, PREC_ADDITIVE, context, expanding)
                ),
                PREC_ADDITIVE,
            )
        }
        UnaryOp::Abs => RenderedExpr::new(
            format!(
                "BigInteger.Abs({})",
                render_expr_prec(operand, 0, context, expanding)
            ),
            PREC_PRIMARY,
        ),
        UnaryOp::Sign => RenderedExpr::new(
            format!(
                "{}.Sign",
                render_expr_prec(operand, PREC_PRIMARY, context, expanding)
            ),
            PREC_PRIMARY,
        ),
    }
}

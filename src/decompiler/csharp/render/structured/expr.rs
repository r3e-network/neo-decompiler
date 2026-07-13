use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::method_body::{SymbolInfo, SymbolOrigin};
use crate::decompiler::csharp::helpers::format_vm_truthiness;
use crate::decompiler::helpers::stack_item_type_tag;
use crate::decompiler::ir::{BinOp, Block, Expr, Intrinsic, Literal, SemanticCallTarget, UnaryOp};
use crate::instruction::OpCode;
use crate::native_contracts;

use super::expr_inline::{is_inline_pure, InlineCollector};

const PREC_ASSIGNMENT: u8 = 1;
const PREC_TERNARY: u8 = 2;
const PREC_BIT_OR: u8 = 5;
const PREC_BIT_XOR: u8 = 6;
const PREC_BIT_AND: u8 = 7;
const PREC_EQUALITY: u8 = 8;
const PREC_RELATIONAL: u8 = 9;
const PREC_SHIFT: u8 = 10;
const PREC_ADDITIVE: u8 = 11;
const PREC_MULTIPLICATIVE: u8 = 12;
const PREC_UNARY: u8 = 13;
const PREC_PRIMARY: u8 = 14;

#[derive(Debug, Default)]
#[cfg_attr(not(test), allow(dead_code))]
pub(super) struct ExprContext {
    inline_values: BTreeMap<String, Expr>,
    value_types: BTreeMap<String, ValueType>,
    emitted_names: BTreeMap<String, String>,
    unpack_packstruct_helper_call: Option<String>,
    tagged_opcode_helper_calls: BTreeMap<(u8, u8), String>,
    internal_call_return_types: BTreeMap<usize, String>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl ExprContext {
    pub(super) fn for_block(
        block: &Block,
        symbols: &BTreeMap<String, SymbolInfo>,
        inline_single_use_temps: bool,
    ) -> Self {
        let value_types = symbols
            .iter()
            .map(|(name, symbol)| (name.clone(), symbol.value_type))
            .collect();
        if !inline_single_use_temps {
            return Self {
                inline_values: BTreeMap::new(),
                value_types,
                emitted_names: BTreeMap::new(),
                unpack_packstruct_helper_call: None,
                tagged_opcode_helper_calls: BTreeMap::new(),
                internal_call_return_types: BTreeMap::new(),
            };
        }

        let mut collector = InlineCollector::default();
        collector.visit_block(block, 0);
        let inline_values = collector
            .definitions
            .iter()
            .filter_map(|(name, definitions)| {
                let [definition] = definitions.as_slice() else {
                    return None;
                };
                let [usage] = collector.uses.get(name)?.as_slice() else {
                    return None;
                };
                let is_typed_temporary = symbols.get(name).is_some_and(|symbol| {
                    symbol.origin == SymbolOrigin::Temporary
                        && matches!(
                            symbol.value_type,
                            ValueType::Integer
                                | ValueType::Boolean
                                | ValueType::ByteString
                                | ValueType::Buffer
                                | ValueType::Array
                                | ValueType::Struct
                                | ValueType::Map
                        )
                });
                (is_typed_temporary
                    && definition.scope == usage.scope
                    && definition.order < usage.order
                    && is_inline_pure(
                        &definition.value,
                        &collector.definitions,
                        definition.order,
                        usage.order,
                        symbols,
                    ))
                .then(|| (name.clone(), definition.value.clone()))
            })
            .collect();
        Self {
            inline_values,
            value_types,
            emitted_names: BTreeMap::new(),
            unpack_packstruct_helper_call: None,
            tagged_opcode_helper_calls: BTreeMap::new(),
            internal_call_return_types: BTreeMap::new(),
        }
    }

    pub(super) fn with_emitted_names(mut self, emitted_names: BTreeMap<String, String>) -> Self {
        self.emitted_names = emitted_names;
        self
    }

    pub(super) fn with_tagged_opcode_helper_calls(
        mut self,
        calls: &BTreeMap<(u8, u8), String>,
    ) -> Self {
        self.tagged_opcode_helper_calls.clone_from(calls);
        self
    }

    pub(super) fn with_unpack_packstruct_helper_call(mut self, call: Option<&str>) -> Self {
        self.unpack_packstruct_helper_call = call.map(str::to_string);
        self
    }

    pub(super) fn with_internal_call_return_types(
        mut self,
        return_types: &BTreeMap<usize, String>,
    ) -> Self {
        self.internal_call_return_types.clone_from(return_types);
        self
    }

    pub(super) fn exact_csharp_type(&self, expression: &Expr) -> Option<&str> {
        let Expr::Call {
            target: SemanticCallTarget::Internal { offset, .. },
            ..
        } = expression
        else {
            return None;
        };
        self.internal_call_return_types
            .get(offset)
            .map(String::as_str)
    }

    pub(super) fn is_inlined(&self, name: &str) -> bool {
        self.inline_values.contains_key(name)
    }

    pub(super) fn value_type(&self, expression: &Expr) -> ValueType {
        match expression {
            Expr::Unknown => ValueType::Unknown,
            Expr::Variable(name) => self
                .value_types
                .get(name)
                .copied()
                .unwrap_or(ValueType::Unknown),
            Expr::Literal(Literal::Int(_) | Literal::BigInt(_)) => ValueType::Integer,
            Expr::Literal(Literal::Bool(_)) => ValueType::Boolean,
            Expr::Literal(Literal::String(_)) => ValueType::ByteString,
            Expr::Literal(Literal::Bytes(_)) => ValueType::ByteString,
            Expr::Literal(Literal::Null) => ValueType::Null,
            Expr::Binary { op, left, right } => match op {
                BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::LogicalAnd
                | BinOp::LogicalOr => ValueType::Boolean,
                _ if self.value_type(left) == ValueType::Integer
                    && self.value_type(right) == ValueType::Integer =>
                {
                    ValueType::Integer
                }
                _ => ValueType::Unknown,
            },
            Expr::Unary { op, operand } => match op {
                UnaryOp::LogicalNot => ValueType::Boolean,
                _ if self.value_type(operand) == ValueType::Integer => ValueType::Integer,
                _ => ValueType::Unknown,
            },
            Expr::Convert { target, .. } => *target,
            Expr::IsType { .. } => ValueType::Boolean,
            Expr::NewArray { .. } | Expr::Array(_) => ValueType::Array,
            Expr::Struct(_) => ValueType::Struct,
            Expr::Map(_) => ValueType::Map,
            Expr::Call {
                target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
                args,
            } => match opcode {
                OpCode::Newarray0 | OpCode::Newarray | OpCode::NewarrayT => ValueType::Array,
                OpCode::Newstruct0 | OpCode::Newstruct => ValueType::Struct,
                OpCode::Newmap => ValueType::Map,
                OpCode::Newbuffer => ValueType::Buffer,
                OpCode::Size | OpCode::Sqrt | OpCode::Min | OpCode::Max => ValueType::Integer,
                OpCode::Haskey | OpCode::Isnull | OpCode::Istype | OpCode::Nz => ValueType::Boolean,
                OpCode::Cat => args
                    .first()
                    .map_or(ValueType::Unknown, |left| self.value_type(left)),
                _ => ValueType::Unknown,
            },
            Expr::Call {
                target: SemanticCallTarget::Intrinsic(Intrinsic::UnpackPackStruct),
                ..
            } => ValueType::Struct,
            _ => ValueType::Unknown,
        }
    }
}

#[derive(Debug)]
struct RenderedExpr {
    source: String,
    precedence: u8,
}

impl RenderedExpr {
    fn new(source: impl Into<String>, precedence: u8) -> Self {
        Self {
            source: source.into(),
            precedence,
        }
    }

    fn in_context(self, parent_precedence: u8) -> String {
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

fn render_expr_prec(
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
                "new object[] {{ {} }}",
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

fn render_call(
    target: &SemanticCallTarget,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    match target {
        SemanticCallTarget::Internal { name, .. } => RenderedExpr::new(
            format!("{name}({})", render_expr_list(args, context, expanding)),
            PREC_PRIMARY,
        ),
        SemanticCallTarget::MethodToken {
            index,
            name,
            hash_le,
            call_flags,
        } => render_method_token_call(
            *index,
            name,
            hash_le.as_deref(),
            *call_flags,
            args,
            context,
            expanding,
        ),
        SemanticCallTarget::Unresolved { display_name } => RenderedExpr::new(
            format!(
                "__NeoDecompilerUnresolvedCall(\"{}\", new object[] {{ {} }})",
                escape_csharp_string(display_name),
                render_expr_list(args, context, expanding)
            ),
            PREC_PRIMARY,
        ),
        SemanticCallTarget::Syscall { hash, .. } => render_syscall(*hash, args, context, expanding),
        SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)) => {
            render_intrinsic(*opcode, args, context, expanding)
        }
        SemanticCallTarget::Intrinsic(Intrinsic::UnpackPackStruct) => {
            let helper = context
                .unpack_packstruct_helper_call
                .as_deref()
                .unwrap_or(super::super::UNPACK_PACKSTRUCT_HELPER);
            RenderedExpr::new(
                format!("{helper}({})", render_expr_list(args, context, expanding)),
                PREC_PRIMARY,
            )
        }
    }
}

fn render_method_token_call(
    index: usize,
    name: &str,
    hash_le: Option<&str>,
    call_flags: Option<u8>,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let bytes = hash_le.and_then(|hash| {
        (hash.len() == 40)
            .then(|| {
                hash.as_bytes()
                    .chunks_exact(2)
                    .map(|pair| {
                        std::str::from_utf8(pair)
                            .ok()
                            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .flatten()
    });
    let (Some(bytes), Some(call_flags)) = (bytes, call_flags) else {
        return RenderedExpr::new(
            format!(
                "__NeoDecompilerUnresolvedCall(\"method token {index}: {}\", new object[] {{ {} }})",
                escape_csharp_string(name),
                render_expr_list(args, context, expanding)
            ),
            PREC_PRIMARY,
        );
    };
    let native_hash = (bytes.len() == 20).then(|| {
        let mut hash = [0u8; 20];
        hash.copy_from_slice(&bytes);
        hash
    });
    if let Some(hint) = native_hash
        .as_ref()
        .and_then(|hash| native_contracts::describe_method_token(hash, name))
        .filter(|hint| hint.has_exact_method() && call_flags == 0x0F)
    {
        let method = hint
            .canonical_method
            .expect("exact native method hint has a canonical name");
        return RenderedExpr::new(
            format!(
                "{}.{method}({})",
                hint.contract,
                render_expr_list(args, context, expanding)
            ),
            PREC_PRIMARY,
        );
    }
    let bytes = bytes
        .iter()
        .map(|byte| format!("0x{byte:02X}"))
        .collect::<Vec<_>>()
        .join(", ");
    RenderedExpr::new(
        format!(
            "(dynamic)Contract.Call((UInt160)new byte[] {{ {bytes} }}, \"{}\", (CallFlags)0x{call_flags:02X}, new object[] {{ {} }})",
            escape_csharp_string(name),
            render_expr_list(args, context, expanding)
        ),
        PREC_PRIMARY,
    )
}

fn render_syscall(
    hash: u32,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let args = syscall_arguments(hash, args);
    match known_syscall_api(hash) {
        Some(SyscallApi::StaticMethod { api, arguments }) => {
            if let Some(arguments) = render_syscall_arguments(args, arguments, context, expanding) {
                return RenderedExpr::new(format!("{api}({arguments})"), PREC_PRIMARY);
            }
        }
        Some(SyscallApi::StaticProperty(api)) => {
            if args.is_empty() {
                return RenderedExpr::new(api, PREC_PRIMARY);
            }
        }
        Some(SyscallApi::InstanceMethod {
            receiver_type,
            method,
            arguments,
        }) => {
            let Some((receiver, rest)) = args.split_first() else {
                return render_low_level_syscall(hash, args, context, expanding);
            };
            if let Some(arguments) = render_syscall_arguments(rest, arguments, context, expanding) {
                let receiver = render_typed_receiver(receiver, receiver_type, context, expanding);
                return RenderedExpr::new(
                    format!("{receiver}.{method}({arguments})"),
                    PREC_PRIMARY,
                );
            }
        }
        Some(SyscallApi::InstanceProperty {
            receiver_type,
            property,
        }) => {
            if let [receiver] = args {
                let receiver = render_typed_receiver(receiver, receiver_type, context, expanding);
                return RenderedExpr::new(format!("{receiver}.{property}"), PREC_PRIMARY);
            }
        }
        Some(SyscallApi::LowLevel) | None => {}
    }

    let rendered = render_low_level_syscall(hash, args, context, expanding);
    if hash == 0x8CEC_27F8 {
        RenderedExpr::new(format!("(bool){}", rendered.source), PREC_UNARY)
    } else {
        rendered
    }
}

fn render_syscall_arguments(
    expressions: &[Expr],
    arguments: &[SyscallArgument],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> Option<String> {
    if expressions.len() != arguments.len() {
        return None;
    }

    expressions
        .iter()
        .zip(arguments)
        .map(|(expression, argument)| {
            let rendered = render_expr_prec(expression, 0, context, expanding);
            Some(match argument {
                SyscallArgument::Cast(target_type) => {
                    format!("({target_type})({rendered})")
                }
                SyscallArgument::Int => format!("(int)({rendered})"),
                SyscallArgument::LongInteger => {
                    format!("(long)(BigInteger)({rendered})")
                }
                SyscallArgument::Enum(target_type) => {
                    format!("({target_type})(int)({rendered})")
                }
                SyscallArgument::StorageKey => match context.value_type(expression) {
                    ValueType::Buffer => format!("(byte[])({rendered})"),
                    ValueType::ByteString => format!("(ByteString)({rendered})"),
                    _ => return None,
                },
                SyscallArgument::StorageValue => match context.value_type(expression) {
                    ValueType::Integer => format!("(BigInteger)({rendered})"),
                    ValueType::Buffer => format!("(byte[])({rendered})"),
                    ValueType::ByteString => format!("(ByteString)({rendered})"),
                    _ => return None,
                },
                SyscallArgument::Witness => match expression {
                    Expr::Cast { target_type, .. }
                        if matches!(target_type.as_str(), "UInt160" | "ECPoint") =>
                    {
                        rendered
                    }
                    _ => return None,
                },
            })
        })
        .collect::<Option<Vec<_>>>()
        .map(|arguments| arguments.join(", "))
}

fn render_typed_receiver(
    expression: &Expr,
    target_type: &str,
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> String {
    format!(
        "(({target_type}){})",
        render_expr_prec(expression, PREC_UNARY, context, expanding)
    )
}

fn render_low_level_syscall(
    hash: u32,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let bytes = std::iter::once(OpCode::Syscall.byte())
        .chain(hash.to_le_bytes())
        .map(|byte| format!("0x{byte:02X}"))
        .collect::<Vec<_>>()
        .join(", ");
    RenderedExpr::new(
        format!(
            "Runtime.LoadScript((ByteString)new byte[] {{ {bytes} }}, CallFlags.All, new object[] {{ {} }})",
            render_expr_list(args, context, expanding)
        ),
        PREC_PRIMARY,
    )
}

fn syscall_arguments(hash: u32, args: &[Expr]) -> &[Expr] {
    let Some(Expr::Literal(Literal::String(metadata))) = args.first() else {
        return args;
    };
    let catalog = crate::syscalls::lookup(hash);
    let has_catalog_selector = catalog.is_some_and(|info| {
        args.len() == usize::from(info.param_count) + 1
            && (metadata == info.name || metadata == &format!("0x{hash:08X}"))
    });
    let has_unknown_selector = catalog.is_none() && metadata == &format!("0x{hash:08X}");
    if has_catalog_selector || has_unknown_selector {
        &args[1..]
    } else {
        args
    }
}

#[derive(Clone, Copy)]
enum SyscallApi {
    StaticMethod {
        api: &'static str,
        arguments: &'static [SyscallArgument],
    },
    StaticProperty(&'static str),
    InstanceMethod {
        receiver_type: &'static str,
        method: &'static str,
        arguments: &'static [SyscallArgument],
    },
    InstanceProperty {
        receiver_type: &'static str,
        property: &'static str,
    },
    LowLevel,
}

#[derive(Clone, Copy)]
enum SyscallArgument {
    Cast(&'static str),
    Int,
    LongInteger,
    Enum(&'static str),
    StorageKey,
    StorageValue,
    Witness,
}

fn known_syscall_api(hash: u32) -> Option<SyscallApi> {
    Some(match hash {
        0x0287_99CF => SyscallApi::StaticMethod {
            api: "Contract.CreateStandardAccount",
            arguments: &[SyscallArgument::Cast("ECPoint")],
        },
        0x0388_C3B7 => SyscallApi::StaticProperty("Runtime.Time"),
        0x09E9_336A => SyscallApi::StaticMethod {
            api: "Contract.CreateMultisigAccount",
            arguments: &[SyscallArgument::Int, SyscallArgument::Cast("ECPoint[]")],
        },
        0x0AE3_0C39 => SyscallApi::StaticMethod {
            api: "Storage.Put",
            arguments: &[SyscallArgument::StorageKey, SyscallArgument::StorageValue],
        },
        0x165D_A144 => SyscallApi::LowLevel,
        0x1DBF_54F3 => SyscallApi::InstanceProperty {
            receiver_type: "Iterator",
            property: "Value",
        },
        0x27B3_E756 => SyscallApi::StaticMethod {
            api: "Crypto.CheckSig",
            arguments: &[
                SyscallArgument::Cast("ECPoint"),
                SyscallArgument::Cast("ByteString"),
            ],
        },
        0x28A9_DE6B => SyscallApi::StaticMethod {
            api: "Runtime.GetRandom",
            arguments: &[],
        },
        0x3008_512D => SyscallApi::StaticProperty("Runtime.Transaction"),
        0x31E8_5D92 => SyscallApi::StaticMethod {
            api: "Storage.Get",
            arguments: &[
                SyscallArgument::Cast("StorageContext"),
                SyscallArgument::StorageKey,
            ],
        },
        0x38E2_B4F9 => SyscallApi::StaticProperty("Runtime.EntryScriptHash"),
        0x3ADC_D09E => SyscallApi::StaticMethod {
            api: "Crypto.CheckMultisig",
            arguments: &[
                SyscallArgument::Cast("ECPoint[]"),
                SyscallArgument::Cast("ByteString[]"),
            ],
        },
        0x3C6E_5339 => SyscallApi::StaticProperty("Runtime.CallingScriptHash"),
        0x4311_2784 => SyscallApi::StaticProperty("Runtime.InvocationCounter"),
        0x525B_7D62 => SyscallApi::StaticMethod {
            api: "Contract.Call",
            arguments: &[
                SyscallArgument::Cast("UInt160"),
                SyscallArgument::Cast("string"),
                SyscallArgument::Enum("CallFlags"),
                SyscallArgument::Cast("object[]"),
            ],
        },
        0x616F_0195 => SyscallApi::LowLevel,
        0x677B_F71A => SyscallApi::LowLevel,
        0x74A8_FEDB => SyscallApi::StaticProperty("Runtime.ExecutingScriptHash"),
        0x813A_DA95 => SyscallApi::StaticMethod {
            api: "Contract.GetCallFlags",
            arguments: &[],
        },
        0x8418_3FE6 => SyscallApi::StaticMethod {
            api: "Storage.Put",
            arguments: &[
                SyscallArgument::Cast("StorageContext"),
                SyscallArgument::StorageKey,
                SyscallArgument::StorageValue,
            ],
        },
        0x8B18_F1AC => SyscallApi::StaticMethod {
            api: "Runtime.CurrentSigners",
            arguments: &[],
        },
        0x8CEC_27F8 => SyscallApi::StaticMethod {
            api: "Runtime.CheckWitness",
            arguments: &[SyscallArgument::Witness],
        },
        0x8F80_0CB3 => SyscallApi::StaticMethod {
            api: "Runtime.LoadScript",
            arguments: &[
                SyscallArgument::Cast("ByteString"),
                SyscallArgument::Enum("CallFlags"),
                SyscallArgument::Cast("object[]"),
            ],
        },
        0x93BC_DB2E => SyscallApi::LowLevel,
        0x94F5_5475 => SyscallApi::StaticMethod {
            api: "Storage.Delete",
            arguments: &[SyscallArgument::StorageKey],
        },
        0x9647_E7CF => SyscallApi::StaticMethod {
            api: "Runtime.Log",
            arguments: &[SyscallArgument::Cast("string")],
        },
        0x9AB8_30DF => SyscallApi::StaticMethod {
            api: "Storage.Find",
            arguments: &[
                SyscallArgument::Cast("StorageContext"),
                SyscallArgument::StorageKey,
                SyscallArgument::Enum("FindOptions"),
            ],
        },
        0x9CED_089C => SyscallApi::InstanceMethod {
            receiver_type: "Iterator",
            method: "Next",
            arguments: &[],
        },
        0xA038_7DE9 => SyscallApi::StaticProperty("Runtime.Trigger"),
        0xBC8C_5AC3 => SyscallApi::StaticMethod {
            api: "Runtime.BurnGas",
            arguments: &[SyscallArgument::LongInteger],
        },
        0xCE67_F69B => SyscallApi::StaticProperty("Storage.CurrentContext"),
        0xCED8_8814 => SyscallApi::StaticProperty("Runtime.GasLeft"),
        0xDC92_494C => SyscallApi::StaticProperty("Runtime.AddressVersion"),
        0xE0A0_FBC5 => SyscallApi::StaticMethod {
            api: "Runtime.GetNetwork",
            arguments: &[],
        },
        0xE26B_B4F6 => SyscallApi::StaticProperty("Storage.CurrentReadOnlyContext"),
        0xE85E_8DD5 => SyscallApi::StaticMethod {
            api: "Storage.Get",
            arguments: &[SyscallArgument::StorageKey],
        },
        0xE9BF_4C76 => SyscallApi::InstanceProperty {
            receiver_type: "StorageContext",
            property: "AsReadOnly",
        },
        0xEDC5_582F => SyscallApi::StaticMethod {
            api: "Storage.Delete",
            arguments: &[
                SyscallArgument::Cast("StorageContext"),
                SyscallArgument::StorageKey,
            ],
        },
        0xF135_4327 => SyscallApi::StaticMethod {
            api: "Runtime.GetNotifications",
            arguments: &[SyscallArgument::Cast("UInt160")],
        },
        0xF352_7607 => SyscallApi::StaticMethod {
            api: "Storage.Find",
            arguments: &[
                SyscallArgument::StorageKey,
                SyscallArgument::Enum("FindOptions"),
            ],
        },
        0xF6FC_79B2 => SyscallApi::StaticProperty("Runtime.Platform"),
        _ => return None,
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn known_syscall_is_classified(hash: u32) -> bool {
    known_syscall_api(hash).is_some()
}

fn render_intrinsic(
    opcode: OpCode,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let arg_at = |index: usize, precedence: u8, expanding: &mut BTreeSet<String>| {
        args.get(index)
            .map(|value| render_expr_prec(value, precedence, context, expanding))
            .unwrap_or_else(|| "default".to_string())
    };
    let arg = |index: usize, expanding: &mut BTreeSet<String>| arg_at(index, 0, expanding);
    let receiver = |index: usize, expanding: &mut BTreeSet<String>| {
        args.get(index)
            .map(|value| render_expr_prec(value, PREC_PRIMARY, context, expanding))
            .unwrap_or_else(|| "default".to_string())
    };
    let call = |name: &str, expanding: &mut BTreeSet<String>| {
        RenderedExpr::new(
            format!("{name}({})", render_expr_list(args, context, expanding)),
            PREC_PRIMARY,
        )
    };

    match opcode {
        OpCode::Within => call("Helper.Within", expanding),
        OpCode::Substr => render_byte_slice(opcode, args, context, expanding, "Helper.Range", true),
        OpCode::Modmul => call("Helper.ModMultiply", expanding),
        OpCode::Modpow => call("BigInteger.ModPow", expanding),
        OpCode::Sqrt => call("Helper.Sqrt", expanding),
        OpCode::Nz => RenderedExpr::new(
            format!("(BigInteger)(dynamic)({}) != 0", arg_at(0, 0, expanding)),
            PREC_EQUALITY,
        ),
        OpCode::Size => {
            match args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value))
            {
                ValueType::Map => {
                    RenderedExpr::new(format!("{}.Count", receiver(0, expanding)), PREC_PRIMARY)
                }
                ValueType::Array
                | ValueType::Struct
                | ValueType::Buffer
                | ValueType::ByteString => {
                    RenderedExpr::new(format!("{}.Length", receiver(0, expanding)), PREC_PRIMARY)
                }
                _ => {
                    let rendered = render_low_level_opcode(opcode, args, context, expanding);
                    RenderedExpr::new(format!("(BigInteger){}", rendered.source), PREC_UNARY)
                }
            }
        }
        OpCode::Keys => RenderedExpr::new(format!("{}.Keys", receiver(0, expanding)), PREC_PRIMARY),
        OpCode::Values => {
            RenderedExpr::new(format!("{}.Values", receiver(0, expanding)), PREC_PRIMARY)
        }
        OpCode::Isnull => {
            if args.first().is_some_and(|value| {
                matches!(
                    context.value_type(value),
                    ValueType::Boolean | ValueType::Integer
                )
            }) {
                RenderedExpr::new("false", PREC_PRIMARY)
            } else {
                RenderedExpr::new(
                    format!("{} is null", arg_at(0, PREC_RELATIONAL, expanding)),
                    PREC_RELATIONAL,
                )
            }
        }
        OpCode::Newbuffer => RenderedExpr::new(
            format!(
                "new byte[{}]",
                args.first()
                    .map(|value| int_cast(value, context, expanding))
                    .unwrap_or_else(|| "default".to_string())
            ),
            PREC_PRIMARY,
        ),
        OpCode::Cat => render_byte_concat(args, context, expanding),
        OpCode::Left | OpCode::Right => render_byte_slice(
            opcode,
            args,
            context,
            expanding,
            if opcode == OpCode::Left {
                "Helper.Take"
            } else {
                "Helper.Last"
            },
            false,
        ),
        OpCode::Min | OpCode::Max => {
            let name = if opcode == OpCode::Min {
                "BigInteger.Min"
            } else {
                "BigInteger.Max"
            };
            call(name, expanding)
        }
        OpCode::Newarray0 => RenderedExpr::new("new object[0]", PREC_PRIMARY),
        OpCode::Newarray | OpCode::NewarrayT | OpCode::Newstruct => RenderedExpr::new(
            format!(
                "new object[{}]",
                args.first()
                    .map(|value| int_cast(value, context, expanding))
                    .unwrap_or_else(|| "default".to_string())
            ),
            PREC_PRIMARY,
        ),
        OpCode::Newstruct0 => RenderedExpr::new("new object[] { }", PREC_PRIMARY),
        OpCode::Newmap => RenderedExpr::new("new Map<object, object>()", PREC_PRIMARY),
        OpCode::Haskey => RenderedExpr::new(
            format!("{}.HasKey({})", receiver(0, expanding), arg(1, expanding)),
            PREC_PRIMARY,
        ),
        OpCode::Pickitem => {
            let receiver_type = args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value));
            let index = if matches!(
                receiver_type,
                ValueType::Array | ValueType::Struct | ValueType::Buffer | ValueType::ByteString
            ) {
                args.get(1)
                    .map(|value| int_cast(value, context, expanding))
                    .unwrap_or_else(|| "default".to_string())
            } else {
                arg(1, expanding)
            };
            let receiver = if matches!(
                receiver_type,
                ValueType::Array
                    | ValueType::Struct
                    | ValueType::Buffer
                    | ValueType::ByteString
                    | ValueType::Map
            ) {
                receiver(0, expanding)
            } else {
                format!("((dynamic)({}))", arg(0, expanding))
            };
            RenderedExpr::new(format!("{receiver}[{index}]"), PREC_PRIMARY)
        }
        OpCode::Setitem => {
            let receiver_type = args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value));
            if matches!(receiver_type, ValueType::Buffer | ValueType::ByteString) {
                let buffer = args.first().map_or_else(
                    || "default".to_string(),
                    |value| {
                        let rendered = render_expr_prec(value, PREC_PRIMARY, context, expanding);
                        if receiver_type == ValueType::ByteString {
                            format!("((byte[])({rendered}))")
                        } else {
                            rendered
                        }
                    },
                );
                let index = args
                    .get(1)
                    .map(|value| int_cast(value, context, expanding))
                    .unwrap_or_else(|| "default".to_string());
                let value = args.get(2).map_or_else(
                    || "default".to_string(),
                    |value| {
                        format!(
                            "(byte)(dynamic)({})",
                            render_expr_prec(value, 0, context, expanding)
                        )
                    },
                );
                RenderedExpr::new(format!("{buffer}[{index}] = {value}"), PREC_ASSIGNMENT)
            } else if matches!(receiver_type, ValueType::Array | ValueType::Struct) {
                let index = args
                    .get(1)
                    .map(|value| int_cast(value, context, expanding))
                    .unwrap_or_else(|| "default".to_string());
                RenderedExpr::new(
                    format!(
                        "{}[{index}] = {}",
                        receiver(0, expanding),
                        arg(2, expanding)
                    ),
                    PREC_ASSIGNMENT,
                )
            } else if receiver_type == ValueType::Map {
                RenderedExpr::new(
                    format!(
                        "{}[{}] = {}",
                        receiver(0, expanding),
                        arg(1, expanding),
                        arg(2, expanding)
                    ),
                    PREC_ASSIGNMENT,
                )
            } else {
                RenderedExpr::new(
                    format!(
                        "((dynamic)({}))[{}] = {}",
                        receiver(0, expanding),
                        arg(1, expanding),
                        arg(2, expanding)
                    ),
                    PREC_ASSIGNMENT,
                )
            }
        }
        OpCode::Append => RenderedExpr::new(
            format!(
                "((Neo.SmartContract.Framework.List<object>){}).Add({})",
                arg(0, expanding),
                arg(1, expanding)
            ),
            PREC_PRIMARY,
        ),
        OpCode::Remove => {
            let receiver_type = args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value));
            if receiver_type == ValueType::Map {
                RenderedExpr::new(
                    format!("{}.Remove({})", receiver(0, expanding), arg(1, expanding)),
                    PREC_PRIMARY,
                )
            } else if matches!(receiver_type, ValueType::Array | ValueType::Struct) {
                let index = args
                    .get(1)
                    .map(|value| int_cast(value, context, expanding))
                    .unwrap_or_else(|| "default".to_string());
                RenderedExpr::new(
                    format!(
                        "((Neo.SmartContract.Framework.List<object>){}).RemoveAt({index})",
                        arg(0, expanding)
                    ),
                    PREC_PRIMARY,
                )
            } else {
                render_low_level_opcode(opcode, args, context, expanding)
            }
        }
        OpCode::Clearitems => {
            let receiver_type = args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value));
            if matches!(receiver_type, ValueType::Array | ValueType::Struct) {
                RenderedExpr::new(
                    format!(
                        "((Neo.SmartContract.Framework.List<object>){}).Clear()",
                        arg(0, expanding)
                    ),
                    PREC_PRIMARY,
                )
            } else if receiver_type == ValueType::Map {
                RenderedExpr::new(format!("{}.Clear()", receiver(0, expanding)), PREC_PRIMARY)
            } else {
                render_low_level_opcode(opcode, args, context, expanding)
            }
        }
        OpCode::Reverseitems => RenderedExpr::new(
            format!("Helper.Reverse({})", arg(0, expanding)),
            PREC_PRIMARY,
        ),
        OpCode::Popitem => RenderedExpr::new(
            format!(
                "((Neo.SmartContract.Framework.List<object>){}).PopItem()",
                arg(0, expanding)
            ),
            PREC_PRIMARY,
        ),
        OpCode::Memcpy => render_memcpy(args, context, expanding),
        OpCode::Convert => {
            RenderedExpr::new(format!("(object)({})", arg(0, expanding)), PREC_UNARY)
        }
        OpCode::Istype => RenderedExpr::new(
            format!("{} is object", arg_at(0, PREC_RELATIONAL, expanding)),
            PREC_RELATIONAL,
        ),
        _ => render_low_level_opcode(opcode, args, context, expanding),
    }
}

fn render_byte_concat(
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let Some(left) = args.first() else {
        return render_low_level_opcode(OpCode::Cat, args, context, expanding);
    };
    let Some(right) = args.get(1) else {
        return render_low_level_opcode(OpCode::Cat, args, context, expanding);
    };
    let left_type = context.value_type(left);
    if matches!(left_type, ValueType::Unknown | ValueType::Any) {
        return render_low_level_opcode(OpCode::Cat, args, context, expanding);
    }

    let left = render_expr_prec(left, 0, context, expanding);
    let left = match left_type {
        ValueType::ByteString => format!("(ByteString)({left})"),
        ValueType::Buffer => format!("(byte[])({left})"),
        _ => format!("(ByteString)(dynamic)({left})"),
    };
    let right_type = context.value_type(right);
    let right = render_expr_prec(right, 0, context, expanding);
    let right = match right_type {
        ValueType::Boolean
        | ValueType::Array
        | ValueType::Struct
        | ValueType::Map
        | ValueType::InteropInterface
        | ValueType::Pointer
        | ValueType::Null => format!("(ByteString)(dynamic)({right})"),
        _ => format!("(ByteString)({right})"),
    };
    RenderedExpr::new(format!("Helper.Concat({left}, {right})"), PREC_PRIMARY)
}

fn render_byte_slice(
    opcode: OpCode,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
    api: &str,
    has_length: bool,
) -> RenderedExpr {
    let Some(source) = args.first() else {
        return render_low_level_opcode(opcode, args, context, expanding);
    };
    let source_type = context.value_type(source);
    if !matches!(source_type, ValueType::ByteString | ValueType::Buffer) {
        return render_low_level_opcode(opcode, args, context, expanding);
    }

    let source = render_expr_prec(source, 0, context, expanding);
    let source = if source_type == ValueType::ByteString {
        format!("(byte[])(ByteString)({source})")
    } else {
        format!("(byte[])({source})")
    };
    let index = args
        .get(1)
        .map(|value| int_cast(value, context, expanding))
        .unwrap_or_else(|| "default".to_string());
    let rendered = if has_length {
        let length = args
            .get(2)
            .map(|value| int_cast(value, context, expanding))
            .unwrap_or_else(|| "default".to_string());
        format!("{api}({source}, {index}, {length})")
    } else {
        format!("{api}({source}, {index})")
    };

    if source_type == ValueType::ByteString {
        RenderedExpr::new(format!("(ByteString)({rendered})"), PREC_UNARY)
    } else {
        RenderedExpr::new(rendered, PREC_PRIMARY)
    }
}

fn render_memcpy(
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let [destination, destination_index, source, source_index, count] = args else {
        return render_low_level_opcode(OpCode::Memcpy, args, context, expanding);
    };
    if context.value_type(destination) != ValueType::Buffer
        || !matches!(
            context.value_type(source),
            ValueType::ByteString | ValueType::Buffer
        )
    {
        return render_low_level_opcode(OpCode::Memcpy, args, context, expanding);
    }

    RenderedExpr::new(
        format!(
            "Array.Copy((byte[])({}), {}, (byte[])({}), {}, {})",
            render_expr_prec(source, 0, context, expanding),
            int_cast(source_index, context, expanding),
            render_expr_prec(destination, 0, context, expanding),
            int_cast(destination_index, context, expanding),
            int_cast(count, context, expanding)
        ),
        PREC_PRIMARY,
    )
}

fn render_low_level_opcode(
    opcode: OpCode,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    RenderedExpr::new(
        format!(
            "Runtime.LoadScript((ByteString)new byte[] {{ 0x{:02X} }}, CallFlags.All, new object[] {{ {} }})",
            opcode.byte(),
            render_expr_list(args, context, expanding)
        ),
        PREC_PRIMARY,
    )
}

fn render_tagged_type_opcode(
    opcode: OpCode,
    target: ValueType,
    value: &Expr,
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let value = render_expr_prec(value, 0, context, expanding);
    render_tagged_type_opcode_source(opcode, target, &value, context)
}

fn render_tagged_type_opcode_source(
    opcode: OpCode,
    target: ValueType,
    value: &str,
    context: &ExprContext,
) -> RenderedExpr {
    let helper = tagged_opcode_helper_key(opcode, target)
        .and_then(|key| context.tagged_opcode_helper_calls.get(&key))
        .cloned()
        .unwrap_or_else(|| default_tagged_opcode_helper_name(opcode, target));
    RenderedExpr::new(format!("{helper}({value})"), PREC_PRIMARY)
}

pub(in crate::decompiler::csharp::render) fn tagged_opcode_helper_key(
    opcode: OpCode,
    target: ValueType,
) -> Option<(u8, u8)> {
    Some((opcode.byte(), stack_item_type_tag(target)?))
}

pub(in crate::decompiler::csharp::render) fn default_tagged_opcode_helper_name(
    opcode: OpCode,
    target: ValueType,
) -> String {
    let operation = match opcode {
        OpCode::Convert => "Convert",
        OpCode::Istype => "IsType",
        _ => "Opcode",
    };
    let target = match target {
        ValueType::Unknown => "Unknown",
        ValueType::Any => "Any",
        ValueType::Null => "Null",
        ValueType::Boolean => "Boolean",
        ValueType::Integer => "Integer",
        ValueType::ByteString => "ByteString",
        ValueType::Buffer => "Buffer",
        ValueType::Array => "Array",
        ValueType::Struct => "Struct",
        ValueType::Map => "Map",
        ValueType::InteropInterface => "InteropInterface",
        ValueType::Pointer => "Pointer",
    };
    format!("__NeoDecompiler{operation}{target}")
}

fn render_low_level_boolean_binary_opcode(
    opcode: OpCode,
    left: &Expr,
    right: &Expr,
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let args = [left.clone(), right.clone()];
    let rendered = render_low_level_opcode(opcode, &args, context, expanding);
    RenderedExpr::new(format!("(bool){}", rendered.source), PREC_UNARY)
}

fn render_expr_list(
    expressions: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> String {
    expressions
        .iter()
        .map(|expression| render_expr_prec(expression, 0, context, expanding))
        .collect::<Vec<_>>()
        .join(", ")
}

fn int_cast(expression: &Expr, context: &ExprContext, expanding: &mut BTreeSet<String>) -> String {
    format!(
        "(int)({})",
        render_expr_prec(expression, 0, context, expanding)
    )
}

fn render_new_array(
    length: &Expr,
    element_type: Option<ValueType>,
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> String {
    let length = int_cast(length, context, expanding);
    match element_type.unwrap_or(ValueType::Any) {
        ValueType::Boolean => format!("new bool[{length}]"),
        ValueType::Integer => format!("new BigInteger[{length}]"),
        ValueType::ByteString => format!("new ByteString[{length}]"),
        ValueType::Buffer => format!("new byte[{length}][]"),
        ValueType::Array | ValueType::Struct => format!("new object[{length}][]"),
        ValueType::Map => format!("new Map<object, object>[{length}]"),
        ValueType::Unknown
        | ValueType::Any
        | ValueType::Null
        | ValueType::InteropInterface
        | ValueType::Pointer => format!("new object[{length}]"),
    }
}

fn render_literal(literal: &Literal) -> String {
    match literal {
        Literal::Int(value) => value.to_string(),
        Literal::BigInt(value) => {
            format!("BigInteger.Parse(\"{}\")", escape_csharp_string(value))
        }
        Literal::Bool(value) => value.to_string(),
        Literal::String(value) => format!("\"{}\"", escape_csharp_string(value)),
        Literal::Bytes(bytes) => format!(
            "(ByteString)new byte[] {{ {} }}",
            bytes
                .iter()
                .map(|byte| format!("0x{byte:02X}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Literal::Null => "null".to_string(),
    }
}

fn escape_csharp_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\0' => escaped.push_str("\\0"),
            '\u{0007}' => escaped.push_str("\\a"),
            '\u{0008}' => escaped.push_str("\\b"),
            '\u{000C}' => escaped.push_str("\\f"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{000B}' => escaped.push_str("\\v"),
            '\u{2028}' => escaped.push_str("\\u2028"),
            '\u{2029}' => escaped.push_str("\\u2029"),
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            control if control.is_control() => {
                use std::fmt::Write;
                write!(escaped, "\\u{:04X}", u32::from(control)).unwrap();
            }
            other => escaped.push(other),
        }
    }
    escaped
}

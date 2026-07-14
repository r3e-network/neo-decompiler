use std::collections::BTreeMap;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::method_body::{SymbolInfo, SymbolOrigin};
use crate::decompiler::ir::{BinOp, Block, Expr, Intrinsic, Literal, SemanticCallTarget, UnaryOp};
use crate::decompiler::native_method_types;
use crate::decompiler::syscall_types;
use crate::instruction::OpCode;

use super::expr_inline::{is_inline_pure, InlineCollector};

#[derive(Debug, Default)]
#[cfg_attr(not(test), allow(dead_code))]
pub(super) struct ExprContext {
    pub(super) inline_values: BTreeMap<String, Expr>,
    value_types: BTreeMap<String, ValueType>,
    pub(super) emitted_names: BTreeMap<String, String>,
    pub(super) unpack_packstruct_helper_call: Option<String>,
    pub(super) tagged_opcode_helper_calls: BTreeMap<(u8, u8), String>,
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
        match expression {
            Expr::Call {
                target: SemanticCallTarget::Internal { offset, .. },
                ..
            } => self
                .internal_call_return_types
                .get(offset)
                .map(String::as_str),
            Expr::Call {
                target:
                    SemanticCallTarget::MethodToken {
                        name,
                        hash_le,
                        call_flags,
                        ..
                    },
                ..
            } => native_method_types::lookup(hash_le.as_deref(), name, *call_flags)
                .map(|return_type| return_type.csharp_type),
            Expr::Call {
                target: SemanticCallTarget::Syscall { hash, .. },
                ..
            } => syscall_types::lookup(*hash).map(|return_type| return_type.csharp_type),
            _ => None,
        }
    }

    fn exact_internal_call_value_type(&self, expression: &Expr) -> ValueType {
        self.exact_csharp_type(expression)
            .and_then(csharp_type_value_type)
            .unwrap_or(ValueType::Unknown)
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
            Expr::Cast { target_type, .. } => {
                csharp_type_value_type(target_type).unwrap_or(ValueType::Unknown)
            }
            Expr::Convert { target, .. } => *target,
            Expr::IsType { .. } => ValueType::Boolean,
            Expr::NewArray { .. } | Expr::Array(_) => ValueType::Array,
            Expr::Struct(_) => ValueType::Struct,
            Expr::Map(_) => ValueType::Map,
            Expr::Index { base, .. } => match base.as_ref() {
                Expr::NewArray {
                    element_type: Some(element_type),
                    ..
                } => *element_type,
                _ if matches!(
                    self.value_type(base),
                    ValueType::ByteString | ValueType::Buffer
                ) =>
                {
                    ValueType::Integer
                }
                _ => ValueType::Unknown,
            },
            Expr::Member { name, .. } if name.eq_ignore_ascii_case("Length") => ValueType::Integer,
            Expr::Member { .. } => ValueType::Unknown,
            Expr::Ternary {
                then_expr,
                else_expr,
                ..
            } => exact_common_value_type(self.value_type(then_expr), self.value_type(else_expr)),
            Expr::Call {
                target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
                args,
            } => match opcode {
                OpCode::Newarray0 | OpCode::Newarray | OpCode::NewarrayT => ValueType::Array,
                OpCode::Keys | OpCode::Values => ValueType::Array,
                OpCode::Newstruct0 | OpCode::Newstruct => ValueType::Struct,
                OpCode::Newmap => ValueType::Map,
                OpCode::Newbuffer => ValueType::Buffer,
                OpCode::Depth | OpCode::Size | OpCode::Sqrt | OpCode::Min | OpCode::Max => {
                    ValueType::Integer
                }
                OpCode::Haskey | OpCode::Isnull | OpCode::Istype | OpCode::Nz => ValueType::Boolean,
                OpCode::Substr | OpCode::Left | OpCode::Right => {
                    args.first().map_or(ValueType::Unknown, |source| {
                        match self.value_type(source) {
                            ValueType::ByteString => ValueType::ByteString,
                            ValueType::Buffer => ValueType::Buffer,
                            _ => ValueType::Unknown,
                        }
                    })
                }
                OpCode::Cat => {
                    args.first()
                        .map_or(ValueType::Unknown, |left| match self.value_type(left) {
                            ValueType::ByteString => ValueType::ByteString,
                            ValueType::Buffer => ValueType::Buffer,
                            _ => ValueType::Unknown,
                        })
                }
                _ => ValueType::Unknown,
            },
            Expr::Call {
                target: SemanticCallTarget::Intrinsic(Intrinsic::UnpackPackStruct),
                ..
            } => ValueType::Struct,
            Expr::Call {
                target: SemanticCallTarget::Internal { .. },
                ..
            } => self.exact_internal_call_value_type(expression),
            Expr::Call {
                target: SemanticCallTarget::MethodToken { .. },
                ..
            } => self
                .exact_csharp_type(expression)
                .and_then(csharp_type_value_type)
                .unwrap_or(ValueType::Unknown),
            Expr::Call {
                target: SemanticCallTarget::Syscall { .. },
                ..
            } => self
                .exact_csharp_type(expression)
                .and_then(csharp_type_value_type)
                .unwrap_or(ValueType::Unknown),
            _ => ValueType::Unknown,
        }
    }
}

fn csharp_type_value_type(csharp_type: &str) -> Option<ValueType> {
    match csharp_type {
        "BigInteger" => Some(ValueType::Integer),
        "bool" => Some(ValueType::Boolean),
        "string" => Some(ValueType::ByteString),
        "ByteString" => Some(ValueType::ByteString),
        "byte[]" => Some(ValueType::Buffer),
        "object[]" => Some(ValueType::Array),
        "Map<object, object>" => Some(ValueType::Map),
        "UInt160" | "UInt256" | "ECPoint" => Some(ValueType::ByteString),
        "StorageContext" | "Iterator" | "Transaction" => Some(ValueType::InteropInterface),
        _ => None,
    }
}

fn exact_common_value_type(left: ValueType, right: ValueType) -> ValueType {
    if left == right && is_concrete_value_type(left) {
        left
    } else {
        ValueType::Unknown
    }
}

fn is_concrete_value_type(value_type: ValueType) -> bool {
    matches!(
        value_type,
        ValueType::Boolean
            | ValueType::Integer
            | ValueType::ByteString
            | ValueType::Buffer
            | ValueType::Array
            | ValueType::Struct
            | ValueType::Map
    )
}

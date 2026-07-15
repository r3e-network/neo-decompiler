use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::method_body::{SymbolInfo, SymbolOrigin};
use crate::decompiler::csharp::render::events::EventSignatures;
use crate::decompiler::ir::{BinOp, Block, Expr, Intrinsic, Literal, SemanticCallTarget, UnaryOp};
use crate::decompiler::native_method_types;
use crate::decompiler::syscall_types;
use crate::instruction::OpCode;

use super::expr_inline::{is_inline_pure, InlineCollector};
#[path = "expr_context_patterns.rs"]
mod patterns;
#[path = "expr_context_types.rs"]
mod types;
use patterns::collect_notification_state_targets;

#[derive(Debug, Default)]
#[cfg_attr(not(test), allow(dead_code))]
pub(super) struct ExprContext {
    pub(super) inline_values: BTreeMap<String, Expr>,
    array_values: BTreeMap<String, Vec<Expr>>,
    array_aliases: BTreeMap<String, String>,
    notification_state_targets: BTreeMap<String, String>,
    debug_singleton_array_targets: BTreeSet<String>,
    event_array_targets: BTreeSet<String>,
    event_signatures: EventSignatures,
    value_types: BTreeMap<String, ValueType>,
    concrete_types: BTreeMap<String, String>,
    typed_array_literals: bool,
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
        let mut collector = InlineCollector::default();
        collector.visit_block(block, 0);
        let array_values = types::collect_array_values(&collector);
        let array_aliases = types::collect_array_aliases(&collector);
        let notification_state_targets = collect_notification_state_targets(block);
        let debug_singleton_array_targets = array_values
            .keys()
            .filter(|name| {
                array_values[*name].len() == 1
                    && notification_state_targets
                        .get(*name)
                        .is_some_and(|label| label == "Debug")
            })
            .cloned()
            .collect();
        if !inline_single_use_temps {
            return Self {
                inline_values: BTreeMap::new(),
                array_values,
                array_aliases,
                notification_state_targets,
                debug_singleton_array_targets,
                event_array_targets: BTreeSet::new(),
                event_signatures: BTreeMap::new(),
                value_types,
                concrete_types: BTreeMap::new(),
                typed_array_literals: false,
                emitted_names: BTreeMap::new(),
                unpack_packstruct_helper_call: None,
                tagged_opcode_helper_calls: BTreeMap::new(),
                internal_call_return_types: BTreeMap::new(),
            };
        }

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
            array_values,
            array_aliases,
            notification_state_targets,
            debug_singleton_array_targets,
            event_array_targets: BTreeSet::new(),
            event_signatures: BTreeMap::new(),
            value_types,
            concrete_types: BTreeMap::new(),
            typed_array_literals: false,
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

    pub(super) fn with_concrete_types(mut self, concrete_types: &BTreeMap<String, String>) -> Self {
        self.concrete_types.clone_from(concrete_types);
        self
    }

    pub(super) fn with_typed_array_literals(mut self, enabled: bool) -> Self {
        self.typed_array_literals = enabled;
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

    pub(super) fn with_event_signatures(mut self, signatures: &EventSignatures) -> Self {
        self.event_signatures.clone_from(signatures);
        self.event_array_targets = self
            .array_values
            .keys()
            .filter(|name| {
                let Some(label) = self.notification_state_targets.get(*name) else {
                    return false;
                };
                signatures.get(label).is_some_and(|(_, parameter_types)| {
                    self.array_values[*name].len() == parameter_types.len()
                })
            })
            .cloned()
            .collect();
        self
    }

    pub(super) fn exact_csharp_type(&self, expression: &Expr) -> Option<&str> {
        match expression {
            Expr::Variable(name) => self.concrete_types.get(name).map(String::as_str),
            Expr::Array(elements) if self.typed_array_literals => {
                let element_types = elements.iter().map(|element| {
                    self.exact_csharp_type(element)
                        .map(str::to_string)
                        .or_else(|| concrete_value_type(self.value_type(element)))
                });
                types::homogeneous_csharp_array_type(element_types)
            }
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
            Expr::Call {
                target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Pickitem)),
                args,
            } => args
                .first()
                .and_then(|base| self.exact_csharp_type(base))
                .and_then(types::csharp_array_element_type),
            Expr::Index { base, .. } => self
                .exact_csharp_type(base)
                .and_then(types::csharp_array_element_type),
            Expr::Member { base, name } => self
                .exact_csharp_type(base)
                .and_then(|base_type| types::csharp_member_type(base_type, name)),
            _ => None,
        }
    }

    fn exact_internal_call_value_type(&self, expression: &Expr) -> ValueType {
        self.exact_csharp_type(expression)
            .and_then(types::csharp_type_value_type)
            .unwrap_or(ValueType::Unknown)
    }

    pub(super) fn is_inlined(&self, name: &str) -> bool {
        self.inline_values.contains_key(name)
    }

    pub(super) fn singleton_array_element<'a>(&'a self, expression: &'a Expr) -> Option<&'a Expr> {
        self.array_elements(expression)
            .and_then(|elements| elements.first().filter(|_| elements.len() == 1))
    }

    pub(super) fn array_elements<'a>(&'a self, expression: &'a Expr) -> Option<&'a [Expr]> {
        match expression {
            Expr::Array(elements) => Some(elements),
            Expr::Variable(name) => {
                let mut current = name.as_str();
                let mut seen = BTreeSet::new();
                loop {
                    if !seen.insert(current) {
                        return None;
                    }
                    if let Some(elements) = self.array_values.get(current) {
                        return Some(elements.as_slice());
                    }
                    current = self.array_aliases.get(current)?.as_str();
                }
            }
            _ => None,
        }
    }

    pub(super) fn event_signature(&self, label: &str) -> Option<(&str, &[String])> {
        self.event_signatures
            .get(label)
            .map(|(name, types)| (name.as_str(), types.as_slice()))
    }

    pub(super) fn is_debug_singleton_array_target(&self, name: &str) -> bool {
        self.debug_singleton_array_targets.contains(name)
    }

    pub(super) fn is_event_array_target(&self, name: &str) -> bool {
        self.event_array_targets.contains(name)
    }

    pub(super) fn value_type(&self, expression: &Expr) -> ValueType {
        match expression {
            Expr::Unknown => ValueType::Unknown,
            Expr::Variable(name) => self
                .concrete_types
                .get(name)
                .filter(|type_name| !matches!(type_name.as_str(), "dynamic" | "object"))
                .and_then(|type_name| types::csharp_type_value_type(type_name))
                .or_else(|| self.value_types.get(name).copied())
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
                types::csharp_type_value_type(target_type).unwrap_or(ValueType::Unknown)
            }
            Expr::Convert { target, .. } => *target,
            Expr::IsType { .. } => ValueType::Boolean,
            Expr::NewArray { .. } | Expr::Array(_) => ValueType::Array,
            Expr::Struct(_) => ValueType::Struct,
            Expr::Map(_) => ValueType::Map,
            Expr::Index { base, .. } => {
                if let Expr::NewArray {
                    element_type: Some(element_type),
                    ..
                } = base.as_ref()
                {
                    *element_type
                } else if let Some(element_type) = self
                    .exact_csharp_type(base)
                    .and_then(types::csharp_array_element_value_type)
                {
                    element_type
                } else if matches!(
                    self.value_type(base),
                    ValueType::ByteString | ValueType::Buffer
                ) {
                    ValueType::Integer
                } else {
                    ValueType::Unknown
                }
            }
            Expr::Member { name, .. } if name.eq_ignore_ascii_case("Length") => ValueType::Integer,
            Expr::Member { .. } => self
                .exact_csharp_type(expression)
                .and_then(types::csharp_type_value_type)
                .unwrap_or(ValueType::Unknown),
            Expr::Ternary {
                then_expr,
                else_expr,
                ..
            } => types::exact_common_value_type(
                self.value_type(then_expr),
                self.value_type(else_expr),
            ),
            Expr::Call {
                target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
                args,
            } => match opcode {
                OpCode::Newarray0 | OpCode::Newarray | OpCode::NewarrayT => ValueType::Array,
                OpCode::Keys | OpCode::Values => ValueType::Array,
                OpCode::Newstruct0 | OpCode::Newstruct => ValueType::Struct,
                OpCode::Newmap => ValueType::Map,
                OpCode::Newbuffer => ValueType::Buffer,
                OpCode::Within => ValueType::Boolean,
                OpCode::Depth | OpCode::Size | OpCode::Sqrt | OpCode::Min | OpCode::Max => {
                    ValueType::Integer
                }
                OpCode::Modmul | OpCode::Modpow => ValueType::Integer,
                OpCode::Haskey | OpCode::Isnull | OpCode::Istype | OpCode::Nz => ValueType::Boolean,
                OpCode::Pickitem => args.first().map_or(ValueType::Unknown, |base| {
                    if let Expr::NewArray {
                        element_type: Some(element_type),
                        ..
                    } = base
                    {
                        return *element_type;
                    }
                    if let Some(element_type) = self
                        .exact_csharp_type(base)
                        .and_then(types::csharp_array_element_value_type)
                    {
                        return element_type;
                    }
                    match self.value_type(base) {
                        ValueType::ByteString | ValueType::Buffer => ValueType::Integer,
                        _ => ValueType::Unknown,
                    }
                }),
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
                .and_then(types::csharp_type_value_type)
                .unwrap_or(ValueType::Unknown),
            Expr::Call {
                target: SemanticCallTarget::Syscall { .. },
                ..
            } => self
                .exact_csharp_type(expression)
                .and_then(types::csharp_type_value_type)
                .unwrap_or(ValueType::Unknown),
            _ => ValueType::Unknown,
        }
    }
}

fn concrete_value_type(value_type: ValueType) -> Option<String> {
    let type_name = types::csharp_type(value_type, true);
    (!type_name.eq_ignore_ascii_case("dynamic")).then(|| type_name.to_string())
}

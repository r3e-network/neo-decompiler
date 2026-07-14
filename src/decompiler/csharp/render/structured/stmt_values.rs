use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::ir::{Expr, Intrinsic, SemanticCallTarget, Stmt, UnaryOp};
use crate::instruction::OpCode;

use super::super::expr::render_expr;
use super::super::plan::{csharp_type, DeclarationKind, ScopeId};
use super::{line, typed_array_csharp_type, StatementRenderer};

impl StatementRenderer<'_> {
    pub(super) fn render_for_initializer(&self, statement: &Stmt) -> String {
        match statement {
            Stmt::Assign { target, value } => self.render_assignment(target, value, false),
            Stmt::ExprStmt(expression) => render_expr(expression, &self.expressions),
            _ => String::new(),
        }
    }

    pub(super) fn render_for_update(&self, expression: &Expr) -> String {
        match expression {
            Expr::Unary {
                op: UnaryOp::Inc,
                operand,
            } => format!("{}++", render_expr(operand, &self.expressions)),
            Expr::Unary {
                op: UnaryOp::Dec,
                operand,
            } => format!("{}--", render_expr(operand, &self.expressions)),
            _ => self
                .render_expression_statement(expression)
                .trim_end_matches(';')
                .to_string(),
        }
    }

    pub(super) fn render_expression_statement(&self, expression: &Expr) -> String {
        let rendered = render_expr(expression, &self.expressions);
        let is_statement_expression = match expression {
            Expr::Call {
                target: SemanticCallTarget::Internal { .. } | SemanticCallTarget::Unresolved { .. },
                ..
            } => true,
            Expr::Call {
                target: SemanticCallTarget::MethodToken { .. },
                ..
            } => false,
            Expr::Call {
                target: SemanticCallTarget::Syscall { hash, .. },
                ..
            } => !crate::syscalls::returns_value(*hash),
            Expr::Call {
                target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
                ..
            } => matches!(
                opcode,
                OpCode::Memcpy
                    | OpCode::Append
                    | OpCode::Setitem
                    | OpCode::Reverseitems
                    | OpCode::Remove
                    | OpCode::Clearitems
            ),
            Expr::Call {
                target: SemanticCallTarget::Intrinsic(Intrinsic::UnpackPackStruct),
                ..
            } => false,
            _ => false,
        };

        if is_statement_expression {
            format!("{rendered};")
        } else {
            format!("_ = {rendered};")
        }
    }

    pub(super) fn render_exception(&self, exception: &str, value: Option<&Expr>) -> String {
        value.map_or_else(
            || format!("throw new {exception}();"),
            |value| {
                format!(
                    "throw new {exception}(Convert.ToString({}));",
                    render_expr(value, &self.expressions)
                )
            },
        )
    }

    pub(super) fn render_vm_throw(&self, value: Option<&Expr>) -> String {
        let payload = value.map_or_else(
            || "null".to_string(),
            |value| render_expr(value, &self.expressions),
        );
        format!("throw new {}({payload});", self.vm_exception_type)
    }

    pub(super) fn render_return(&self, value: &Expr) -> String {
        let rendered = self.render_typed_value(value, self.return_type.unwrap_or("dynamic"));
        format!("return {rendered};")
    }

    pub(super) fn render_assignment(&self, target: &str, value: &Expr, semicolon: bool) -> String {
        let target_type = if self.plan.typed && self.plan.index_defined_symbols.contains(target) {
            "dynamic"
        } else {
            self.plan
                .declarations
                .get(target)
                .map(|declaration| declaration.csharp_type.as_str())
                .or_else(|| self.plan.static_field_types.get(target).map(String::as_str))
                .unwrap_or_else(|| {
                    if self.plan.typed {
                        csharp_type(self.expressions.value_type(&Expr::var(target)), true)
                    } else {
                        "dynamic"
                    }
                })
        };
        let value = self.render_typed_value(value, target_type);
        let body = match self.plan.declarations.get(target) {
            Some(declaration) if declaration.kind == DeclarationKind::Inline => format!(
                "{} {} = {value}",
                declaration.csharp_type, declaration.emitted_name
            ),
            Some(declaration) => format!("{} = {value}", declaration.emitted_name),
            None => format!("{target} = {value}"),
        };
        if semicolon {
            format!("{body};")
        } else {
            body
        }
    }

    pub(super) fn render_typed_value(&self, value: &Expr, target_type: &str) -> String {
        let rendered = render_expr(value, &self.expressions);
        if self.expressions.exact_csharp_type(value) == Some(target_type) {
            return rendered;
        }
        if let Expr::Variable(name) = value {
            let source_type = self
                .plan
                .declarations
                .get(name)
                .map(|declaration| declaration.csharp_type.as_str())
                .or_else(|| self.plan.static_field_types.get(name).map(String::as_str));
            if source_type == Some(target_type) {
                return rendered;
            }
        }
        let source_type = self.expressions.value_type(value);
        if target_type == "object[]"
            && source_type == ValueType::Array
            && matches!(
                self.expressions.exact_csharp_type(value),
                Some("ECPoint[]" | "Signer[]")
            )
        {
            return rendered;
        }
        let cast = match (target_type, source_type) {
            ("dynamic" | "object", _) => None,
            (_, ValueType::Unknown | ValueType::Any | ValueType::Null) => {
                Some(format!("({target_type})(dynamic)({rendered})"))
            }
            ("ByteString", ValueType::Buffer | ValueType::Integer) => {
                Some(format!("(ByteString)({rendered})"))
            }
            ("byte[]", ValueType::ByteString) => Some(format!("(byte[])({rendered})")),
            ("BigInteger", ValueType::ByteString) => Some(format!("(BigInteger)({rendered})")),
            ("UInt160" | "UInt256" | "ECPoint", ValueType::ByteString) => {
                Some(format!("({target_type})(byte[])({rendered})"))
            }
            ("UInt160" | "UInt256" | "ECPoint", ValueType::Buffer) => {
                Some(format!("({target_type})({rendered})"))
            }
            ("string", ValueType::ByteString) => None,
            (target_type, ValueType::Array)
                if matches!(
                    value,
                    Expr::NewArray {
                        element_type: Some(element_type),
                        ..
                    } if target_type == typed_array_csharp_type(*element_type)
                ) =>
            {
                None
            }
            ("object[]", ValueType::Array | ValueType::Struct)
                if matches!(
                    value,
                    Expr::NewArray {
                        element_type: Some(_),
                        ..
                    }
                ) =>
            {
                Some(format!("(object[])(dynamic)({rendered})"))
            }
            ("object[]", ValueType::Array | ValueType::Struct)
                if matches!(
                    value,
                    Expr::Variable(name)
                        if self.plan.declarations.get(name).is_some_and(|declaration| {
                            declaration.csharp_type != "object[]"
                        })
                ) =>
            {
                Some(format!("(object[])(dynamic)({rendered})"))
            }
            ("BigInteger", ValueType::Integer)
            | ("bool", ValueType::Boolean)
            | ("ByteString", ValueType::ByteString)
            | ("byte[]", ValueType::Buffer)
            | ("object[]", ValueType::Array | ValueType::Struct)
            | ("Map<object, object>", ValueType::Map) => None,
            _ => Some(format!("({target_type})(dynamic)({rendered})")),
        };
        cast.unwrap_or(rendered)
    }

    pub(super) fn hoisted_declarations(&self, scope: ScopeId, indent: usize) -> Vec<String> {
        self.plan
            .declarations
            .values()
            .filter(|declaration| {
                declaration.scope == scope && declaration.kind == DeclarationKind::HoistedAssignment
            })
            .map(|declaration| {
                let initializer = if declaration.initialize_to_default {
                    " = default"
                } else {
                    ""
                };
                line(
                    indent,
                    format!(
                        "{} {}{initializer};",
                        declaration.csharp_type, declaration.emitted_name
                    ),
                )
            })
            .collect()
    }
}

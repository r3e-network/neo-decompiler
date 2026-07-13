use std::collections::HashSet;

use crate::manifest::ManifestParameter;

pub(super) use super::super::helpers::make_unique_identifier;
use super::super::helpers::sanitize_identifier;

#[derive(Clone)]
pub(super) struct CSharpParameter {
    pub(super) name: String,
    pub(super) ty: String,
}

pub(super) fn collect_csharp_parameters(parameters: &[ManifestParameter]) -> Vec<CSharpParameter> {
    let mut used_names = HashSet::new();
    parameters
        .iter()
        .map(|param| CSharpParameter {
            name: make_unique_identifier(sanitize_csharp_identifier(&param.name), &mut used_names),
            ty: format_manifest_type_csharp(&param.kind, false),
        })
        .collect()
}

pub(super) fn format_csharp_parameters(params: &[CSharpParameter]) -> String {
    params
        .iter()
        .map(|param| format!("{} {}", param.ty, param.name))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn format_manifest_type_csharp(kind: &str, for_return: bool) -> String {
    match kind.to_ascii_lowercase().as_str() {
        // `void` is only legal in return position. In a parameter / event-arg
        // position it would render the illegal `void` / `Action<void>`
        // (C# error CS1547), so fall through to the `object` default there.
        "void" if for_return => "void".into(),
        "boolean" | "bool" => "bool".into(),
        "integer" | "int" => "BigInteger".into(),
        "string" => "string".into(),
        "hash160" => "UInt160".into(),
        "hash256" => "UInt256".into(),
        "publickey" => "ECPoint".into(),
        "bytearray" | "bytes" => "ByteString".into(),
        "signature" => "ByteString".into(),
        "array" => "object[]".into(),
        "map" => "object".into(),
        "interopinterface" => "object".into(),
        "any" => "object".into(),
        _ => "object".into(),
    }
}

pub(super) fn format_method_signature(name: &str, parameters: &str, return_type: &str) -> String {
    if parameters.is_empty() {
        format!("public static {return_type} {name}()")
    } else {
        format!("public static {return_type} {name}({parameters})")
    }
}

pub(super) const VM_ASSERT_MESSAGE_HELPER: &str = "__NeoDecompilerAssertMessage";
pub(super) const VM_EXCEPTION_TYPE: &str = "__NeoDecompilerVmException";

#[cfg(test)]
pub(in crate::decompiler) fn legacy_statement_to_csharp(line: &str) -> String {
    legacy_statement_to_csharp_with_context(
        line,
        &SlotTypes::default(),
        false,
        VM_ASSERT_MESSAGE_HELPER,
    )
}

/// Inferred C# type strings for the argument, local, and static slots visible
/// while rendering one method.
///
/// Built from [`crate::decompiler::analysis::types::TypeInfo`] for the method
/// being rendered. Entries are empty (`""`) when no type was inferred, in which
/// case the declaration falls back to `var`.
#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub(in crate::decompiler) struct SlotTypes {
    /// C# type name per argument-slot index, or `""` when unknown.
    pub arguments: Vec<&'static str>,
    /// Emitted parameter name per argument-slot index.
    pub argument_names: Vec<String>,
    /// C# type name per local-slot index, or `""` when unknown.
    pub locals: Vec<&'static str>,
    /// C# type name per static-field-slot index, or `""` when unknown.
    pub statics: Vec<&'static str>,
}

#[cfg(test)]
impl SlotTypes {
    /// Resolve the C# declaration type for a slot name emitted by the lifter
    /// (`arg0`, a named parameter, `loc3`, or `static1`). Returns a type only
    /// when inference or the manifest supplied one.
    fn declaration_type(&self, name: &str) -> Option<&'static str> {
        let slot_type = self
            .argument_names
            .iter()
            .position(|argument| argument == name)
            .and_then(|index| self.arguments.get(index))
            .or_else(|| {
                name.strip_prefix("arg")
                    .and_then(parse_slot_index)
                    .and_then(|index| self.arguments.get(index))
            })
            .or_else(|| {
                name.strip_prefix("loc")
                    .and_then(parse_slot_index)
                    .and_then(|index| self.locals.get(index))
            })
            .or_else(|| {
                name.strip_prefix("static")
                    .and_then(parse_slot_index)
                    .and_then(|index| self.statics.get(index))
            });
        slot_type.copied().filter(|ty| !ty.is_empty())
    }
}

/// Parse the trailing digit run of a slot name (`"3"` from `"loc3"`) as a slot
/// index. Returns `None` unless the remainder is all-ASCII-digits so that
/// unrelated identifiers never accidentally match.
#[cfg(test)]
fn parse_slot_index(rest: &str) -> Option<usize> {
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse::<usize>().ok()
}

/// Rewrite one lifted statement with method type and helper context.
#[cfg(test)]
pub(in crate::decompiler) fn legacy_statement_to_csharp_with_context(
    line: &str,
    types: &SlotTypes,
    typed_declarations: bool,
    assert_message_helper: &str,
) -> String {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("//") {
        return trimmed.to_string();
    }
    if let Some(stripped) = trimmed.strip_prefix("let ") {
        // The declared name is the first token of the initialiser (up to the
        // first whitespace or `=`); consult the inferred-type map and prefer a
        // concrete C# type over `var` when one is known.
        let name = stripped
            .split(|c: char| c.is_whitespace() || c == '=')
            .next()
            .unwrap_or("");
        let body = legacy_expression_to_csharp(stripped);
        if typed_declarations {
            if let Some(ty) = types.declaration_type(name) {
                return format!("{ty} {body}");
            }
        }
        return format!("var {body}");
    }
    legacy_statement_to_csharp_untyped(trimmed, types, assert_message_helper)
}

/// Shared line-level rewrites after declaration handling.
#[cfg(test)]
fn legacy_statement_to_csharp_untyped(
    trimmed: &str,
    types: &SlotTypes,
    assert_message_helper: &str,
) -> String {
    if trimmed == "loop {" {
        return "while (true) {".to_string();
    }
    if let Some(condition) = trimmed
        .strip_prefix("if ")
        .and_then(|rest| rest.strip_suffix(" {"))
    {
        return format!("if ({}) {{", legacy_expression_to_csharp(condition.trim()));
    }
    if let Some(condition) = trimmed
        .strip_prefix("else if ")
        .and_then(|rest| rest.strip_suffix(" {"))
    {
        return format!(
            "else if ({}) {{",
            legacy_expression_to_csharp(condition.trim())
        );
    }
    if let Some(condition) = trimmed
        .strip_prefix("} else if ")
        .and_then(|rest| rest.strip_suffix(" {"))
    {
        return format!(
            "}} else if ({}) {{",
            legacy_expression_to_csharp(condition.trim())
        );
    }
    if let Some(condition) = trimmed
        .strip_prefix("while ")
        .and_then(|rest| rest.strip_suffix(" {"))
    {
        return format!(
            "while ({}) {{",
            legacy_expression_to_csharp(condition.trim())
        );
    }
    if trimmed.starts_with("for (") && trimmed.ends_with(" {") {
        let inner = &trimmed[4..trimmed.len() - 2];
        let inner = inner.strip_prefix('(').unwrap_or(inner);
        let inner = inner.strip_suffix(')').unwrap_or(inner);
        let converted = inner.replacen("let ", "var ", 1);
        return format!("for ({}) {{", legacy_expression_to_csharp(&converted));
    }
    if let Some(scrutinee) = trimmed
        .strip_prefix("switch ")
        .and_then(|rest| rest.strip_suffix(" {"))
    {
        return format!(
            "switch ({}) {{",
            legacy_expression_to_csharp(scrutinee.trim())
        );
    }
    if let Some(value) = trimmed
        .strip_prefix("case ")
        .and_then(|rest| rest.strip_suffix(" {"))
    {
        return format!("case {}: {{", value.trim());
    }
    if trimmed == "default {" {
        return "default: {".to_string();
    }
    if let Some(target) = trimmed.strip_prefix("leave ") {
        return format!("goto {target}");
    }
    // Neo can throw any stack value, while C# exception payloads are strings.
    // Match the structured renderer's explicit, single-evaluation coercion.
    if let Some(rest) = trimmed
        .strip_prefix("throw(")
        .and_then(|r| r.strip_suffix(");"))
    {
        let operand = legacy_expression_to_csharp(rest);
        return format!("throw new Exception(Convert.ToString({operand}));");
    }
    // C# cannot model an uncatchable VM abort. Keep it distinct from THROW so
    // the conservative translation remains visible in every renderer path.
    if trimmed == "abort();" {
        return "throw new InvalidOperationException();".to_string();
    }
    if let Some(rest) = trimmed
        .strip_prefix("abort(")
        .and_then(|r| r.strip_suffix(");"))
    {
        let operand = legacy_expression_to_csharp(rest);
        return format!("throw new InvalidOperationException(Convert.ToString({operand}));");
    }
    // ASSERT uses the framework intrinsic; ASSERTMSG uses the generated direct
    // opcode helper so its message remains eagerly validated. Casts supply C#
    // parameter types without changing the VM stack items.
    if let Some(args) = trimmed
        .strip_prefix("assert(")
        .and_then(|r| r.strip_suffix(");"))
    {
        if let Some((cond, message)) = split_top_level_comma(args) {
            let message_expr = legacy_expression_to_csharp(message.trim());
            return format_vm_assertion(
                &csharpize_vm_condition(cond, types),
                Some(&message_expr),
                assert_message_helper,
            );
        }
        return format_vm_assertion(
            &csharpize_vm_condition(args, types),
            None,
            assert_message_helper,
        );
    }
    legacy_expression_to_csharp(trimmed)
}

#[cfg(test)]
fn csharpize_vm_condition(condition: &str, types: &SlotTypes) -> String {
    let condition = condition.trim();
    let rendered = legacy_expression_to_csharp(condition);
    if condition == "null" {
        "false".to_string()
    } else if is_decimal_integer_literal(condition)
        || types.declaration_type(condition) == Some("BigInteger")
    {
        format!("{rendered} != 0")
    } else if matches!(condition, "true" | "false")
        || types.declaration_type(condition) == Some("bool")
    {
        rendered
    } else {
        format_vm_truthiness(&rendered)
    }
}

/// Give an arbitrary VM value a C# bool type without emitting a VM conversion.
/// Neo.Compiler.CSharp erases these object casts, leaving the raw stack item for
/// ASSERT to evaluate through StackItem.GetBoolean().
pub(super) fn format_vm_truthiness(expression: &str) -> String {
    format!("(bool)(object)({expression})")
}

/// Render native VM assertion APIs. ASSERTMSG uses an opcode-annotated helper:
/// Neo.Compiler.CSharp rewrites its framework overload to lazy JMPIF + ABORTMSG,
/// which skips the native opcode's eager message validation on success.
pub(super) fn format_vm_assertion(
    condition: &str,
    message: Option<&str>,
    assert_message_helper: &str,
) -> String {
    message.map_or_else(
        || format!("global::Neo.SmartContract.Framework.ExecutionEngine.Assert({condition});"),
        |message| format!("{assert_message_helper}({condition}, (string)(object)({message}));"),
    )
}

#[cfg(test)]
mod legacy_expression;
#[cfg(test)]
use legacy_expression::{
    is_decimal_integer_literal, legacy_expression_to_csharp, split_top_level_comma,
};
pub(super) fn sanitize_csharp_identifier(input: &str) -> String {
    let ident = sanitize_identifier(input);
    if is_csharp_keyword(&ident) {
        format!("@{ident}")
    } else {
        ident
    }
}

fn is_csharp_keyword(ident: &str) -> bool {
    matches!(
        ident,
        "abstract"
            | "as"
            | "base"
            | "bool"
            | "break"
            | "byte"
            | "case"
            | "catch"
            | "char"
            | "checked"
            | "class"
            | "const"
            | "continue"
            | "decimal"
            | "default"
            | "delegate"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "event"
            | "explicit"
            | "extern"
            | "false"
            | "finally"
            | "fixed"
            | "float"
            | "for"
            | "foreach"
            | "goto"
            | "if"
            | "implicit"
            | "in"
            | "int"
            | "interface"
            | "internal"
            | "is"
            | "lock"
            | "long"
            | "namespace"
            | "new"
            | "null"
            | "object"
            | "operator"
            | "out"
            | "override"
            | "params"
            | "private"
            | "protected"
            | "public"
            | "readonly"
            | "ref"
            | "return"
            | "sbyte"
            | "sealed"
            | "short"
            | "sizeof"
            | "stackalloc"
            | "static"
            | "string"
            | "struct"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "uint"
            | "ulong"
            | "unchecked"
            | "unsafe"
            | "ushort"
            | "using"
            | "virtual"
            | "void"
            | "volatile"
            | "while"
            | "add"
            | "alias"
            | "ascending"
            | "async"
            | "await"
            | "by"
            | "descending"
            | "dynamic"
            | "equals"
            | "from"
            | "get"
            | "global"
            | "group"
            | "init"
            | "into"
            | "join"
            | "let"
            | "nameof"
            | "on"
            | "orderby"
            | "partial"
            | "remove"
            | "select"
            | "set"
            | "unmanaged"
            | "value"
            | "var"
            | "when"
            | "where"
            | "with"
            | "yield"
    )
}

pub(super) fn escape_csharp_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

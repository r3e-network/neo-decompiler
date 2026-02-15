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
            ty: format_manifest_type_csharp(&param.kind),
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

pub(super) fn format_manifest_type_csharp(kind: &str) -> String {
    match kind.to_ascii_lowercase().as_str() {
        "void" => "void".into(),
        "boolean" | "bool" => "bool".into(),
        "integer" | "int" => "BigInteger".into(),
        "string" => "string".into(),
        "hash160" => "UInt160".into(),
        "hash256" => "UInt256".into(),
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

pub(in crate::decompiler) fn csharpize_statement(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("//") {
        return trimmed.to_string();
    }
    if let Some(stripped) = trimmed.strip_prefix("let ") {
        return format!("var {stripped}");
    }
    if trimmed.starts_with("if ") && trimmed.ends_with(" {") {
        let condition = trimmed[3..trimmed.len() - 2].trim();
        return format!("if ({condition}) {{");
    }
    if trimmed.starts_with("while ") && trimmed.ends_with(" {") {
        let condition = trimmed[6..trimmed.len() - 2].trim();
        return format!("while ({condition}) {{");
    }
    if trimmed.starts_with("for (") && trimmed.ends_with(" {") {
        let inner = &trimmed[4..trimmed.len() - 2];
        let inner = inner.strip_prefix('(').unwrap_or(inner);
        let inner = inner.strip_suffix(')').unwrap_or(inner);
        let converted = inner.replacen("let ", "var ", 1);
        return format!("for ({converted}) {{");
    }
    if let Some(target) = trimmed.strip_prefix("leave ") {
        return format!("goto {target}");
    }
    trimmed.to_string()
}

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

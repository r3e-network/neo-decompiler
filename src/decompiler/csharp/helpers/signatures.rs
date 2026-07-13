use std::collections::HashSet;

use crate::manifest::ManifestParameter;

pub(in crate::decompiler::csharp) use super::super::super::helpers::make_unique_identifier;
use super::super::super::helpers::sanitize_identifier;

#[derive(Clone)]
pub(in crate::decompiler::csharp) struct CSharpParameter {
    pub(in crate::decompiler::csharp) name: String,
    pub(in crate::decompiler::csharp) ty: String,
}

pub(in crate::decompiler::csharp) fn collect_csharp_parameters(
    parameters: &[ManifestParameter],
) -> Vec<CSharpParameter> {
    let mut used_names = HashSet::new();
    parameters
        .iter()
        .map(|param| CSharpParameter {
            name: make_unique_identifier(sanitize_csharp_identifier(&param.name), &mut used_names),
            ty: format_manifest_type_csharp(&param.kind, false),
        })
        .collect()
}

pub(in crate::decompiler::csharp) fn format_csharp_parameters(
    params: &[CSharpParameter],
) -> String {
    params
        .iter()
        .map(|param| format!("{} {}", param.ty, param.name))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(in crate::decompiler::csharp) fn format_manifest_type_csharp(
    kind: &str,
    for_return: bool,
) -> String {
    match kind.to_ascii_lowercase().as_str() {
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
        "map" | "interopinterface" | "any" => "object".into(),
        _ => "object".into(),
    }
}

pub(in crate::decompiler::csharp) fn format_method_signature(
    name: &str,
    parameters: &str,
    return_type: &str,
) -> String {
    if parameters.is_empty() {
        format!("public static {return_type} {name}()")
    } else {
        format!("public static {return_type} {name}({parameters})")
    }
}

pub(in crate::decompiler::csharp) fn sanitize_csharp_identifier(input: &str) -> String {
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

pub(in crate::decompiler::csharp) fn escape_csharp_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

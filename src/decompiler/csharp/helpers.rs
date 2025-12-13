use crate::manifest::ManifestParameter;

use super::super::helpers::sanitize_identifier;

#[derive(Clone)]
pub(super) struct CSharpParameter {
    pub(super) name: String,
    pub(super) ty: String,
}

pub(super) fn collect_csharp_parameters(parameters: &[ManifestParameter]) -> Vec<CSharpParameter> {
    parameters
        .iter()
        .map(|param| CSharpParameter {
            name: sanitize_identifier(&param.name),
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
    trimmed.to_string()
}

pub(super) fn escape_csharp_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

//! Collection and conversion rewrites for legacy lifted C# expressions.

use super::legacy_expression_to_csharp;
use super::scanner::{
    find_matching_close_paren, matching_paren, split_top_level_args, split_top_level_colon,
};

/// Rewrite a non-empty PACKMAP literal into a C# map collection initializer.
pub(super) fn match_map_literal(rest: &str) -> Option<(String, usize)> {
    let inner = rest.strip_prefix("Map(")?;
    let close = matching_paren(inner)?;
    let body = inner[..close].trim();
    if body.is_empty() || body.contains("/*") {
        return None;
    }
    let mut entries = Vec::new();
    for entry in split_top_level_args(body) {
        let (key, value) = split_top_level_colon(entry)?;
        entries.push(format!(
            "[{}] = {}",
            legacy_expression_to_csharp(key.trim()),
            legacy_expression_to_csharp(value.trim())
        ));
    }
    let consumed = "Map(".len() + close + 1;
    Some((
        format!("new Map<object, object> {{ {} }}", entries.join(", ")),
        consumed,
    ))
}

/// Recognize the empty collection forms emitted by the legacy lift.
pub(super) fn match_collection_constructor(rest: &str) -> Option<(&'static str, usize)> {
    const TABLE: &[(&str, &str)] = &[
        ("Map()", "new Map<object, object>()"),
        ("Struct()", "new Struct()"),
        ("[]", "new object[0]"),
    ];
    for (needle, replacement) in TABLE {
        if rest.starts_with(needle) {
            return Some((replacement, needle.len()));
        }
    }
    None
}

/// Recognize unary, collection, and typed conversion helper forms.
pub(super) fn match_unary_pattern(rest: &str) -> Option<HelperRewrite> {
    if let Some(rendered) = match_simple_unary(rest, "is_null(", |arg| format!("({arg} is null)")) {
        return Some(rendered);
    }
    if let Some(rendered) = match_simple_unary(rest, "new_buffer(", |arg| {
        format!("new byte[{}]", wrap_int_cast_unless_literal(arg))
    }) {
        return Some(rendered);
    }
    if let Some(rendered) = match_simple_unary(rest, "new_array(", |arg| {
        format!("new object[{}]", wrap_int_cast_unless_literal(arg))
    }) {
        return Some(rendered);
    }
    // Collection helpers map to the standard .NET / Neo member forms.
    if let Some(rendered) = match_simple_unary(rest, "clear_items(", |arg| format!("{arg}.Clear()"))
    {
        return Some(rendered);
    }
    if let Some(rendered) = match_simple_unary(rest, "keys(", |arg| format!("{arg}.Keys")) {
        return Some(rendered);
    }
    if let Some(rendered) = match_simple_unary(rest, "values(", |arg| format!("{arg}.Values")) {
        return Some(rendered);
    }
    if let Some(rendered) =
        match_simple_unary(rest, "reverse_items(", |arg| format!("{arg}.Reverse()"))
    {
        return Some(rendered);
    }
    for (needle, method) in METHOD_CALL_TABLE {
        if let Some(rendered) = match_method_call(rest, needle, method) {
            return Some(rendered);
        }
    }
    for (needle, csharp_type) in CONVERT_TYPED_TABLE {
        if let Some(rendered) =
            match_simple_unary(rest, needle, |arg| format!("({csharp_type})({arg})"))
        {
            return Some(rendered);
        }
    }
    for (needle, csharp_type) in IS_TYPE_TYPED_TABLE {
        if let Some(rendered) =
            match_simple_unary(rest, needle, |arg| format!("({arg} is {csharp_type})"))
        {
            return Some(rendered);
        }
    }
    None
}

const METHOD_CALL_TABLE: &[(&str, &str)] = &[
    ("remove_item(", "Remove"),
    ("append(", "Add"),
    ("has_key(", "ContainsKey"),
];

const CONVERT_TYPED_TABLE: &[(&str, &str)] = &[
    ("convert_to_bool(", "bool"),
    ("convert_to_integer(", "BigInteger"),
    ("convert_to_bytestring(", "ByteString"),
    ("convert_to_buffer(", "byte[]"),
];

const IS_TYPE_TYPED_TABLE: &[(&str, &str)] = &[
    ("is_type_bool(", "bool"),
    ("is_type_integer(", "BigInteger"),
    ("is_type_bytestring(", "ByteString"),
    ("is_type_buffer(", "byte[]"),
];

/// Wrap a numeric argument in `(int)(...)`, except for a bare decimal literal.
fn wrap_int_cast_unless_literal(arg: &str) -> String {
    let trimmed = arg.trim();
    if is_decimal_integer_literal(trimmed) {
        trimmed.to_string()
    } else {
        format!("(int)({trimmed})")
    }
}

pub(super) fn is_decimal_integer_literal(text: &str) -> bool {
    !text.is_empty()
        && text
            .strip_prefix('-')
            .unwrap_or(text)
            .chars()
            .all(|ch| ch.is_ascii_digit())
}

fn match_simple_unary(
    rest: &str,
    needle: &str,
    render: impl FnOnce(&str) -> String,
) -> Option<HelperRewrite> {
    if !rest.starts_with(needle) {
        return None;
    }
    let after_open = &rest[needle.len()..];
    let close_index = find_matching_close_paren(after_open.as_bytes())?;
    let arg = after_open[..close_index].trim();
    Some(HelperRewrite {
        body: render(arg),
        consumed: needle.len() + close_index + 1,
    })
}

/// Rewrite a helper call into a C# member call with the first argument as the
/// receiver, for example `remove_item(coll, key)` → `coll.Remove(key)`.
fn match_method_call(rest: &str, needle: &str, method_name: &str) -> Option<HelperRewrite> {
    if !rest.starts_with(needle) {
        return None;
    }
    let after_open = &rest[needle.len()..];
    let close_index = find_matching_close_paren(after_open.as_bytes())?;
    let args = &after_open[..close_index];
    let parts = split_top_level_args(args);
    if parts.is_empty() {
        return None;
    }
    let receiver = parts[0].trim();
    let rest_args = parts[1..]
        .iter()
        .map(|part| part.trim())
        .collect::<Vec<_>>()
        .join(", ");
    Some(HelperRewrite {
        body: format!("{receiver}.{method_name}({rest_args})"),
        consumed: needle.len() + close_index + 1,
    })
}

pub(super) struct HelperRewrite {
    pub(super) body: String,
    pub(super) consumed: usize,
}

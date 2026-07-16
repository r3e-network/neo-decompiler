//! Numeric helper lowering for legacy lifted C# expressions.

use super::collections::{match_collection_constructor, match_map_literal, match_unary_pattern};
use super::legacy_expression_to_csharp;
use super::literals::{match_big_byte_literal, match_big_integer_literal};
use super::scanner::{find_matching_close_paren, split_top_level_args};

/// Rewrite NEO arithmetic and buffer helper calls into C# framework forms.
pub(super) fn rewrite_numeric_helpers(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    let mut in_string: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        // Copy non-ASCII characters as whole UTF-8 code points. Helper names
        // are ASCII, and slicing at a continuation byte would otherwise panic.
        if !b.is_ascii() {
            if line.is_char_boundary(i) {
                let ch = line[i..].chars().next().unwrap_or('\u{FFFD}');
                out.push(ch);
                i += ch.len_utf8();
            } else {
                i += 1;
            }
            continue;
        }
        if let Some(quote) = in_string {
            out.push(b as char);
            if b == b'\\' && i + 1 < bytes.len() {
                let escaped = line[i + 1..].chars().next().unwrap_or('\u{FFFD}');
                out.push(escaped);
                i += 1 + escaped.len_utf8();
                continue;
            }
            if b == quote {
                in_string = None;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_string = Some(b);
            out.push(b as char);
            i += 1;
            continue;
        }
        let prev_is_ident_continuation = i > 0 && {
            let previous = bytes[i - 1];
            previous.is_ascii_alphanumeric() || previous == b'_'
        };
        if !prev_is_ident_continuation {
            if let Some(rendered) = match_unary_pattern(&line[i..]) {
                out.push_str(&rendered.body);
                i += rendered.consumed;
                continue;
            }
            if let Some((replacement, consumed)) = match_collection_constructor(&line[i..]) {
                out.push_str(replacement);
                i += consumed;
                continue;
            }
            if let Some((rendered, consumed)) = match_map_literal(&line[i..]) {
                out.push_str(&rendered);
                i += consumed;
                continue;
            }
            if let Some((rendered, consumed)) = match_big_byte_literal(&line[i..]) {
                out.push_str(&rendered);
                i += consumed;
                continue;
            }
            if let Some((rendered, consumed)) = match_big_integer_literal(&line[i..]) {
                out.push_str(&rendered);
                i += consumed;
                continue;
            }
            if let Some(rule) = match_numeric_helper(&bytes[i..]) {
                if !rule.int_cast_args.is_empty() {
                    if let Some(rendered) = format_helper_with_casts(&line[i..], &rule) {
                        out.push_str(&rendered.body);
                        i += rendered.consumed;
                        continue;
                    }
                }
                out.push_str(rule.replacement);
                out.push('(');
                i += rule.needle_len;
                continue;
            }
        }
        out.push(b as char);
        i += 1;
    }
    out
}

struct HelperRule {
    replacement: &'static str,
    needle_len: usize,
    int_cast_args: &'static [usize],
}

fn match_numeric_helper(bytes: &[u8]) -> Option<HelperRule> {
    const TABLE: &[(&[u8], &str, &[usize])] = &[
        (b"abs(", "BigInteger.Abs", &[]),
        (b"min(", "BigInteger.Min", &[]),
        (b"max(", "BigInteger.Max", &[]),
        (b"pow(", "BigInteger.Pow", &[1]),
        (b"modpow(", "BigInteger.ModPow", &[]),
        (b"sign(", "Helper.Sign", &[]),
        (b"sqrt(", "Helper.Sqrt", &[]),
        (b"modmul(", "Helper.ModMul", &[]),
        (b"within(", "Helper.Within", &[]),
        (b"left(", "Helper.Left", &[1]),
        (b"right(", "Helper.Right", &[1]),
        (b"substr(", "Helper.Substr", &[1, 2]),
    ];
    for (needle, replacement, int_cast_args) in TABLE {
        if bytes.starts_with(needle) {
            return Some(HelperRule {
                replacement,
                needle_len: needle.len(),
                int_cast_args,
            });
        }
    }
    None
}

fn format_helper_with_casts(
    rest: &str,
    rule: &HelperRule,
) -> Option<super::collections::HelperRewrite> {
    let after_open = &rest[rule.needle_len..];
    let close_index = find_matching_close_paren(after_open.as_bytes())?;
    let args = &after_open[..close_index];
    let parts = split_top_level_args(args);
    let mut rendered = Vec::with_capacity(parts.len());
    for (index, part) in parts.iter().enumerate() {
        // Normalize nested helpers before applying the C#-specific cast.
        let normalized = legacy_expression_to_csharp(part.trim());
        if rule.int_cast_args.contains(&index) {
            rendered.push(wrap_int_cast_unless_literal(&normalized));
        } else {
            rendered.push(normalized);
        }
    }
    let body = format!("{}({})", rule.replacement, rendered.join(", "));
    Some(super::collections::HelperRewrite {
        consumed: rule.needle_len + close_index + 1,
        body,
    })
}

fn wrap_int_cast_unless_literal(arg: &str) -> String {
    let trimmed = arg.trim();
    if super::collections::is_decimal_integer_literal(trimmed) {
        trimmed.to_string()
    } else {
        format!("(int)({trimmed})")
    }
}

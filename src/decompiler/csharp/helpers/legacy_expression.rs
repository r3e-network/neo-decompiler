/// Apply the C# expression-level rewrites (cat → `+`, NEO helper
/// calls → `BigInteger.X` / `Helper.X` / pattern forms) to a fragment
/// that's already known to be a single expression — for instance the
/// condition of an `if`/`while`, or the scrutinee of a `switch`.
///
/// The statement rewriter's control-flow branches dispatch to this so
/// the rewrites apply uniformly whether the helper appears in a
/// statement position or inside a control header.
#[cfg(test)]
#[path = "legacy_expression_scanner.rs"]
mod scanner;
#[cfg(test)]
pub(super) use scanner::split_top_level_comma;
#[cfg(test)]
use scanner::{
    find_matching_close_paren, matching_paren, rewrite_cat_operator, split_top_level_args,
    split_top_level_colon,
};

#[cfg(test)]
pub(super) fn legacy_expression_to_csharp(text: &str) -> String {
    rewrite_numeric_helpers(&rewrite_cat_operator(text))
}

/// Rewrite NEO arithmetic / buffer helper calls into compilable C# forms.
///
/// The high-level lift emits these NEO opcodes as bare function calls
/// (`abs(x)`, `min(a, b)`, `pow(a, b)`, `left(s, n)`, etc.). C# has
/// no top-level `abs`/`min`/`pow`/`left` etc. in scope, so leaving
/// them as-is produces output that doesn't compile against the
/// standard NEO SmartContract Framework. We rewrite to two
/// canonical forms:
///
/// - `System.Numerics.BigInteger` static methods (`BigInteger.Abs`,
///   `BigInteger.Min`, `BigInteger.Max`, `BigInteger.Pow`,
///   `BigInteger.ModPow`) — pure .NET, in scope via the
///   `using System.Numerics;` preamble.
/// - `Neo.SmartContract.Framework.Helper` static methods
///   (`Helper.Sign`, `Helper.Sqrt`, `Helper.ModMul`, `Helper.Within`,
///   `Helper.Left`, `Helper.Right`, `Helper.Substr`) — for ones with
///   no `BigInteger` equivalent.
///
/// Some of these accept `int` parameters at specific positions
/// (`BigInteger.Pow`'s exponent; `Left`/`Right`'s `n`; `Substr`'s
/// `start` + `length`). The lift uses `BigInteger` everywhere, so
/// we wrap those args in `(int)(...)` to keep the call
/// type-correct.
///
/// The rewrite is identifier-boundary aware (so `pow(x)` matches
/// but `mypow(x)` does not) and string-aware (so a literal
/// containing `abs(...)` is preserved verbatim).
#[cfg(test)]
fn rewrite_numeric_helpers(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    let mut in_string: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        // Copy any non-ASCII (multibyte UTF-8) character verbatim. A helper
        // pattern can only begin with ASCII, and slicing `&line[i..]` at a
        // multibyte continuation byte would panic (not a char boundary), so
        // advance one whole character at a time here. The `is_char_boundary`
        // guard keeps this panic-free even if an earlier match advanced `i`
        // into the middle of a character.
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
                // Skip the escaped character (so an escaped quote does not end
                // the string), copying it whole in case it is multibyte.
                let esc = line[i + 1..].chars().next().unwrap_or('\u{FFFD}');
                out.push(esc);
                i += 1 + esc.len_utf8();
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
            let p = bytes[i - 1];
            p.is_ascii_alphanumeric() || p == b'_'
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

/// Wrap a bare decimal integer literal that exceeds C#'s `ulong` range in
/// `BigInteger.Parse("…")`. A literal above `ulong.MaxValue` is C# error CS1021
/// ("integral constant is too large"), which the lift hits for large
/// PUSHINT128/PUSHINT256 operands. `System.Numerics` is already imported by the
/// C# preamble, so `BigInteger.Parse` is in scope.
///
/// The caller guarantees `s` begins at an identifier boundary (so digits inside
/// `t12`/`loc5` are never matched). Only the magnitude is wrapped, so a leading
/// unary `-` stays valid (`-BigInteger.Parse("…")`). Hex (`0x…`) and literals
/// that continue into an identifier or a `.` are left untouched.
#[cfg(test)]
fn match_big_integer_literal(s: &str) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_digit() {
        return None;
    }
    let mut j = 0;
    while j < bytes.len() && bytes[j].is_ascii_digit() {
        j += 1;
    }
    if j < bytes.len() {
        let after = bytes[j];
        // `0x…` hex prefix, an identifier continuation, or a fractional/member
        // `.` means this digit run is not a standalone decimal literal.
        if after == b'x'
            || after == b'X'
            || after.is_ascii_alphabetic()
            || after == b'_'
            || after == b'.'
        {
            return None;
        }
    }
    let digits = &s[..j];
    if !decimal_exceeds_u64(digits) {
        return None;
    }
    Some((format!("BigInteger.Parse(\"{digits}\")"), j))
}

/// Rewrite a non-empty PACKMAP literal `Map(k1: v1, k2: v2)` into a C# map
/// collection initializer `new Map<object, object> { [k1] = v1, [k2] = v2 }`.
///
/// The high-level lift renders PACKMAP as `Map(key: value, …)`, whose `:`
/// separators are not valid inside a C# call (error CS1026). The empty `Map()`
/// form is handled by [`match_collection_constructor`]; a body carrying a
/// `/* N more … */` truncation marker is left untouched (it is an incomplete
/// literal that cannot be rendered faithfully). Keys and values are recursively
/// run through [`legacy_expression_to_csharp`] so nested helpers/maps are translated.
#[cfg(test)]
fn match_map_literal(rest: &str) -> Option<(String, usize)> {
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

/// Rewrite an oversized `0x…` hex blob into a C# `new byte[] { … }` literal.
///
/// The high-level lift renders a non-printable PUSHDATA operand as `0x<HEX>`.
/// When that blob is wider than 8 bytes (> 16 hex digits) it cannot be a C#
/// integer literal (above `ulong` → error CS1021). A run that long is also
/// unambiguously a byte blob: every other `0x…` the lift emits — syscall hashes
/// (8 digits), CALLT indices and jump targets (≤ 4 digits) — is shorter, so a
/// length cutoff cannot misfire on them. Short hex is left untouched.
///
/// `new byte[] { … }` compiles wherever a byte array is assignable (an `object`
/// return, an expression operand). A manifest-typed return such as `UInt160`
/// still needs a cast the text rewriter can't infer here, but eliminating the
/// uncompilable integer literal is the correct minimal fix.
#[cfg(test)]
fn match_big_byte_literal(s: &str) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'0' || !(bytes[1] == b'x' || bytes[1] == b'X') {
        return None;
    }
    let mut j = 2;
    while j < bytes.len() && bytes[j].is_ascii_hexdigit() {
        j += 1;
    }
    let hex = &s[2..j];
    // > 16 nibbles (beyond ulong) and whole bytes only.
    if hex.len() <= 16 || hex.len() % 2 != 0 {
        return None;
    }
    // Must be a complete token, not the prefix of a longer identifier.
    if j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
        return None;
    }
    let mut rendered = String::from("new byte[] { ");
    for (idx, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        if idx > 0 {
            rendered.push_str(", ");
        }
        rendered.push_str("0x");
        rendered.push(pair[0] as char);
        rendered.push(pair[1] as char);
    }
    rendered.push_str(" }");
    Some((rendered, j))
}

/// Whether a run of decimal digits represents a value greater than
/// `u64::MAX` (18446744073709551615) — i.e. beyond what a C# `ulong` literal
/// can hold. Compared by length then lexically to avoid overflowing any fixed
/// integer width (PUSHINT256 values can be up to 78 digits).
#[cfg(test)]
fn decimal_exceeds_u64(digits: &str) -> bool {
    const U64_MAX: &str = "18446744073709551615";
    let trimmed = digits.trim_start_matches('0');
    let significant = if trimmed.is_empty() { "0" } else { trimmed };
    match significant.len().cmp(&U64_MAX.len()) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => significant > U64_MAX,
    }
}

#[cfg(test)]
struct HelperRule {
    replacement: &'static str,
    needle_len: usize,
    /// 0-based positions within the call's argument list that need
    /// to be wrapped in `(int)(...)` for the C# overload signature
    /// to match. Empty for helpers that take only `BigInteger`/
    /// reference-typed args.
    int_cast_args: &'static [usize],
}

/// Recognise empty NEWMAP / NEWARRAY0 / NEWSTRUCT0 constructor
/// shapes the lift emits and rewrite to compilable C# forms:
///
/// - `Map()` → `new Map<object, object>()` — the Neo
///   `SmartContract.Framework.Services.Map` is generic; without
///   key/value-type info we default to `object`.
/// - `[]` → `new object[0]` — the bare collection literal `[]`
///   has no inferable target type at the lift site, so emit an
///   explicit zero-length `object[]`.
/// - `Struct()` → `new Struct()` — Neo's runtime `Struct` is a
///   reference type; the empty constructor is the simplest
///   compilable form even if the user later fills items in.
///
/// Returns `(replacement, consumed_bytes)` on a match. Only the
/// empty-arg forms (literally `Map()`, `[]`, `Struct()`) are
/// handled here. Non-empty PACK / PACKMAP / PACKSTRUCT shapes
/// (e.g. `Map(k1, v1, k2, v2)`) are deferred — they need
/// collection-initialiser rendering that the lift doesn't supply
/// the structure for yet.
#[cfg(test)]
fn match_collection_constructor(rest: &str) -> Option<(&'static str, usize)> {
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

/// Recognise lifted unary helpers that don't map cleanly to a
/// `Method(arg)` rewrite — they expand into a small inline
/// expression instead.
///
/// - `is_null(x)` → `(x is null)` (idiomatic C# pattern match;
///   works against any reference-typed argument including the NEO
///   runtime stack types).
/// - `new_buffer(n)` → `new byte[(int)(n)]` — NEWBUFFER lifts to a
///   byte-array constructor in C#.
/// - `new_array(n)` → `new object[(int)(n)]` — NEO arrays are
///   heterogeneous; `object[]` is the safe fallback without
///   element-type info.
#[cfg(test)]
fn match_unary_pattern(rest: &str) -> Option<HelperRewrite> {
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
    // Collection helpers — lift emits `clear_items(c)`, `keys(m)`,
    // `values(m)`, `reverse_items(arr)`. Rewrite to the standard
    // .NET / Neo Map / List accessor forms. These work for both
    // Neo's `Map<TKey, TValue>` and `List<T>` / arrays via the
    // common collection interfaces.
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
    // Two-argument collection helpers — `remove_item(c, k)` lifts
    // to `c.Remove(k)` (works for both `Map<,>` and `List<>` /
    // collection-interface types). `append(arr, item)` →
    // `arr.Add(item)` matches `List<T>.Add` and Neo's `Array.Add`.
    // `has_key(c, k)` → `c.ContainsKey(k)` matches `IDictionary`.
    for (needle, method) in METHOD_CALL_TABLE {
        if let Some(rendered) = match_method_call(rest, needle, method) {
            return Some(rendered);
        }
    }
    // Typed CONVERT and ISTYPE rewrites — the high-level lift emits
    // `convert_to_T(x)` / `is_type_T(x)` for each NEO stack-item
    // type. Map the safe subset (bool / integer / bytestring /
    // buffer) onto C# casts (`(T)(x)`) and pattern matches
    // (`(x is T)`). The other types (any, pointer, array, struct,
    // map, interopinterface) need more context — left as-is so the
    // user sees an obvious "fix this manually" identifier.
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

#[cfg(test)]
const METHOD_CALL_TABLE: &[(&str, &str)] = &[
    ("remove_item(", "Remove"),
    ("append(", "Add"),
    ("has_key(", "ContainsKey"),
];

#[cfg(test)]
const CONVERT_TYPED_TABLE: &[(&str, &str)] = &[
    ("convert_to_bool(", "bool"),
    ("convert_to_integer(", "BigInteger"),
    ("convert_to_bytestring(", "ByteString"),
    ("convert_to_buffer(", "byte[]"),
];

#[cfg(test)]
const IS_TYPE_TYPED_TABLE: &[(&str, &str)] = &[
    ("is_type_bool(", "bool"),
    ("is_type_integer(", "BigInteger"),
    ("is_type_bytestring(", "ByteString"),
    ("is_type_buffer(", "byte[]"),
];

/// Wrap a numeric argument in `(int)(...)` — but skip the cast when
/// the argument is already a literal decimal integer (`3`, `-5`,
/// `100`). Neo's lifted source treats all integers as BigInteger by
/// default, so a defensive `(int)` cast is needed for any expression
/// that could carry BigInteger semantics — but a bare integer
/// literal is unambiguously an `int` to the C# parser, and the
/// redundant cast just adds visual noise (`new object[(int)(3)]`
/// → `new object[3]`).
#[cfg(test)]
fn wrap_int_cast_unless_literal(arg: &str) -> String {
    let trimmed = arg.trim();
    if is_decimal_integer_literal(trimmed) {
        trimmed.to_string()
    } else {
        format!("(int)({trimmed})")
    }
}

#[cfg(test)]
pub(super) fn is_decimal_integer_literal(text: &str) -> bool {
    !text.is_empty()
        && text
            .strip_prefix('-')
            .unwrap_or(text)
            .chars()
            .all(|ch| ch.is_ascii_digit())
}

#[cfg(test)]
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

/// Rewrite a lifted helper call of shape `prefix(arg0, arg1, ...)`
/// into a C# method invocation `arg0.method_name(arg1, ...)`. Used
/// for collection helpers like `remove_item(coll, key)` →
/// `coll.Remove(key)` where the first argument is the receiver.
#[cfg(test)]
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
        .map(|p| p.trim())
        .collect::<Vec<_>>()
        .join(", ");
    Some(HelperRewrite {
        body: format!("{receiver}.{method_name}({rest_args})"),
        consumed: needle.len() + close_index + 1,
    })
}

#[cfg(test)]
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

#[cfg(test)]
struct HelperRewrite {
    body: String,
    consumed: usize,
}

#[cfg(test)]
fn format_helper_with_casts(rest: &str, rule: &HelperRule) -> Option<HelperRewrite> {
    let after_open = &rest[rule.needle_len..];
    let close_index = find_matching_close_paren(after_open.as_bytes())?;
    let args = &after_open[..close_index];
    let parts = split_top_level_args(args);
    let mut rendered = Vec::with_capacity(parts.len());
    for (index, part) in parts.iter().enumerate() {
        // Normalize each argument through the same expression rewriter the
        // non-cast helpers use, so nested NEO helper calls (abs/min/sqrt/
        // is_null/convert_to_*, ` cat `) become compilable C# rather than being
        // emitted verbatim (e.g. `pow(abs(x), 2)` → `BigInteger.Pow(BigInteger.Abs(x), 2)`).
        let normalized = legacy_expression_to_csharp(part.trim());
        if rule.int_cast_args.contains(&index) {
            // Same idea as `wrap_int_cast_unless_literal`: defensive
            // `(int)` casts are necessary for variable / expression
            // operands that could carry BigInteger semantics, but
            // bare integer literals are unambiguously `int` to the C#
            // parser, so emit `pow(2, 8)` → `BigInteger.Pow(2, 8)`
            // rather than `BigInteger.Pow(2, (int)(8))`.
            rendered.push(wrap_int_cast_unless_literal(&normalized));
        } else {
            rendered.push(normalized);
        }
    }
    let body = format!("{}({})", rule.replacement, rendered.join(", "));
    Some(HelperRewrite {
        consumed: rule.needle_len + close_index + 1,
        body,
    })
}

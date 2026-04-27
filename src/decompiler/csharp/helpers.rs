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

pub(in crate::decompiler) fn csharpize_statement(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("//") {
        return trimmed.to_string();
    }
    if let Some(stripped) = trimmed.strip_prefix("let ") {
        // Run the body through `csharpize_expression` so helper
        // calls inside the initialiser also get rewritten — e.g.
        // `let t0 = min(x, y);` must become
        // `var t0 = BigInteger.Min(x, y);`. Without this, the
        // `let` branch was early-returning before the expression
        // rewrites ran (same bug class as the if/while/etc.
        // control-flow branches earlier).
        return format!("var {}", csharpize_expression(stripped));
    }
    if trimmed == "loop {" {
        return "while (true) {".to_string();
    }
    if trimmed.starts_with("if ") && trimmed.ends_with(" {") {
        let condition = trimmed[3..trimmed.len() - 2].trim();
        return format!("if ({}) {{", csharpize_expression(condition));
    }
    if let Some(condition) = trimmed
        .strip_prefix("else if ")
        .and_then(|rest| rest.strip_suffix(" {"))
    {
        return format!("else if ({}) {{", csharpize_expression(condition.trim()));
    }
    if let Some(condition) = trimmed
        .strip_prefix("} else if ")
        .and_then(|rest| rest.strip_suffix(" {"))
    {
        return format!("}} else if ({}) {{", csharpize_expression(condition.trim()));
    }
    if trimmed.starts_with("while ") && trimmed.ends_with(" {") {
        let condition = trimmed[6..trimmed.len() - 2].trim();
        return format!("while ({}) {{", csharpize_expression(condition));
    }
    if trimmed.starts_with("for (") && trimmed.ends_with(" {") {
        let inner = &trimmed[4..trimmed.len() - 2];
        let inner = inner.strip_prefix('(').unwrap_or(inner);
        let inner = inner.strip_suffix(')').unwrap_or(inner);
        let converted = inner.replacen("let ", "var ", 1);
        return format!("for ({}) {{", csharpize_expression(&converted));
    }
    if let Some(scrutinee) = trimmed
        .strip_prefix("switch ")
        .and_then(|rest| rest.strip_suffix(" {"))
    {
        return format!("switch ({}) {{", csharpize_expression(scrutinee.trim()));
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
    // The high-level emitter renders NEO's `THROW` opcode as
    // `throw(value);` — NEO can throw any stack value (int, string,
    // byte[], etc.). `System.Exception`'s constructor takes a
    // `string`, so non-string operands need to be coerced. We
    // detect string-literal operands (already valid) and wrap
    // everything else in `$"{value}"` interpolation, which calls
    // ToString implicitly and always produces a `string`. The
    // operand itself is run through `csharpize_expression` so
    // helper-call rewrites inside (e.g. `throw(BigInteger.Pow(a, b))`)
    // also apply.
    if let Some(rest) = trimmed
        .strip_prefix("throw(")
        .and_then(|r| r.strip_suffix(");"))
    {
        let operand = csharpize_expression(rest);
        return format!(
            "throw new Exception({});",
            wrap_exception_operand_for_csharp(&operand)
        );
    }
    // NEO `ABORT` / `ABORTMSG` are uncatchable VM aborts. C# has no
    // direct equivalent, but `throw new Exception(...)` (uncaught)
    // terminates execution the same way and reads naturally for a
    // post-decompile reader. Bare `abort()` becomes `throw new
    // Exception();`; `abort(msg)` applies the same coercion as
    // `throw(...)`.
    if trimmed == "abort();" {
        return "throw new Exception();".to_string();
    }
    if let Some(rest) = trimmed
        .strip_prefix("abort(")
        .and_then(|r| r.strip_suffix(");"))
    {
        let operand = csharpize_expression(rest);
        return format!(
            "throw new Exception({});",
            wrap_exception_operand_for_csharp(&operand)
        );
    }
    // NEO's `ASSERT` / `ASSERTMSG` are runtime checks: throw if the
    // condition is false. C# has no `assert(...)` keyword/function in
    // scope, so a `assert(cond);` line wouldn't compile. The closest
    // universal form is `if (!(cond)) throw new Exception();` (and
    // `throw new Exception(msg);` when a message is supplied) — works
    // without any helper imports. Both the condition and the message
    // run through `csharpize_expression` so helper rewrites apply.
    if let Some(args) = trimmed
        .strip_prefix("assert(")
        .and_then(|r| r.strip_suffix(");"))
    {
        if let Some((cond, message)) = split_top_level_comma(args) {
            // Same `Exception(string)` coercion rule as throw/abort:
            // string-typed messages pass through, non-string get
            // wrapped in `$"{...}"` interpolation.
            let message_expr = csharpize_expression(message.trim());
            return format!(
                "if (!({})) throw new Exception({});",
                csharpize_expression(cond.trim()),
                wrap_exception_operand_for_csharp(&message_expr)
            );
        }
        return format!(
            "if (!({})) throw new Exception();",
            csharpize_expression(args.trim())
        );
    }
    csharpize_expression(trimmed)
}

/// Coerce a `throw(...)` / `abort(...)` operand into something
/// `new Exception(string)` accepts. NEO bytecode can THROW any
/// stack value; `System.Exception`'s only constructor that takes a
/// payload is `Exception(string)`, so non-string operands need to
/// be coerced. The detection is:
///
/// - Self-contained `"…"` string literal → already valid; pass
///   through verbatim.
/// - Contains a `"` somewhere → likely a string concatenation
///   (`"err" + code`, `prefix + "..." + suffix`, etc.). The result
///   is a `string`; pass through.
/// - Otherwise (numeric literals, identifiers without `"`, helper
///   calls returning non-string) → wrap in C# interpolation
///   `$"{value}"`, which calls `ToString()` implicitly and always
///   produces a `string`.
///
/// The "any `"` triggers pass-through" heuristic occasionally
/// false-positives (e.g. `BigInteger.Parse("123") + 1` would slip
/// through and produce `Exception(BigInteger)` — uncompilable). In
/// practice the lift produces `"…"` only inside genuine string
/// constructions, so the heuristic stays cleaner than a full type
/// inference and the user can still hand-edit the rare miss.
fn wrap_exception_operand_for_csharp(operand: &str) -> String {
    let trimmed = operand.trim();
    if operand_appears_string_typed(trimmed) {
        operand.to_string()
    } else {
        format!("$\"{{{trimmed}}}\"")
    }
}

/// Return `true` when `text` plausibly evaluates to a `string`
/// without further coercion — either a single string literal or
/// any expression containing one (most commonly a string-concat).
fn operand_appears_string_typed(text: &str) -> bool {
    text.contains('"')
}

/// Apply the C# expression-level rewrites (cat → `+`, NEO helper
/// calls → `BigInteger.X` / `Helper.X` / pattern forms) to a fragment
/// that's already known to be a single expression — for instance the
/// condition of an `if`/`while`, or the scrutinee of a `switch`.
///
/// `csharpize_statement`'s control-flow branches dispatch to this so
/// the rewrites apply uniformly whether the helper appears in a
/// statement position or inside a control header.
fn csharpize_expression(text: &str) -> String {
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
fn rewrite_numeric_helpers(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    let mut in_string: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(quote) = in_string {
            out.push(b as char);
            if b == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
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

/// Wrap a numeric argument in `(int)(...)` — but skip the cast when
/// the argument is already a literal decimal integer (`3`, `-5`,
/// `100`). Neo's lifted source treats all integers as BigInteger by
/// default, so a defensive `(int)` cast is needed for any expression
/// that could carry BigInteger semantics — but a bare integer
/// literal is unambiguously an `int` to the C# parser, and the
/// redundant cast just adds visual noise (`new object[(int)(3)]`
/// → `new object[3]`).
fn wrap_int_cast_unless_literal(arg: &str) -> String {
    let trimmed = arg.trim();
    let is_decimal_literal = !trimmed.is_empty()
        && trimmed
            .strip_prefix('-')
            .unwrap_or(trimmed)
            .chars()
            .all(|ch| ch.is_ascii_digit());
    if is_decimal_literal {
        trimmed.to_string()
    } else {
        format!("(int)({trimmed})")
    }
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

/// Rewrite a lifted helper call of shape `prefix(arg0, arg1, ...)`
/// into a C# method invocation `arg0.method_name(arg1, ...)`. Used
/// for collection helpers like `remove_item(coll, key)` →
/// `coll.Remove(key)` where the first argument is the receiver.
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

struct HelperRewrite {
    body: String,
    consumed: usize,
}

fn format_helper_with_casts(rest: &str, rule: &HelperRule) -> Option<HelperRewrite> {
    let after_open = &rest[rule.needle_len..];
    let close_index = find_matching_close_paren(after_open.as_bytes())?;
    let args = &after_open[..close_index];
    let parts = split_top_level_args(args);
    let mut rendered = Vec::with_capacity(parts.len());
    for (index, part) in parts.iter().enumerate() {
        let trimmed = part.trim();
        if rule.int_cast_args.contains(&index) {
            // Same idea as `wrap_int_cast_unless_literal`: defensive
            // `(int)` casts are necessary for variable / expression
            // operands that could carry BigInteger semantics, but
            // bare integer literals are unambiguously `int` to the C#
            // parser, so emit `pow(2, 8)` → `BigInteger.Pow(2, 8)`
            // rather than `BigInteger.Pow(2, (int)(8))`.
            rendered.push(wrap_int_cast_unless_literal(trimmed));
        } else {
            rendered.push(trimmed.to_string());
        }
    }
    let body = format!("{}({})", rule.replacement, rendered.join(", "));
    Some(HelperRewrite {
        consumed: rule.needle_len + close_index + 1,
        body,
    })
}

/// Split a top-level argument list — like `split_top_level_comma` but
/// returns every comma-separated piece, not just the first split.
fn split_top_level_args(args: &str) -> Vec<&str> {
    let bytes = args.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut in_string: Option<u8> = None;
    for (i, &b) in bytes.iter().enumerate() {
        if let Some(quote) = in_string {
            if b == b'\\' {
                continue;
            }
            if b == quote {
                in_string = None;
            }
            continue;
        }
        match b {
            b'"' | b'\'' => in_string = Some(b),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(&args[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if !args.is_empty() {
        parts.push(&args[start..]);
    }
    parts
}

fn find_matching_close_paren(bytes: &[u8]) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut in_string: Option<u8> = None;
    for (i, &b) in bytes.iter().enumerate() {
        if let Some(quote) = in_string {
            if b == b'\\' {
                continue;
            }
            if b == quote {
                in_string = None;
            }
            continue;
        }
        match b {
            b'"' | b'\'' => in_string = Some(b),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Split a comma-separated argument list at the first top-level comma —
/// i.e. ignore commas inside parens / brackets / strings. Used by the
/// C#-ize pass to peel `assert(cond, message);` into its two pieces.
fn split_top_level_comma(args: &str) -> Option<(&str, &str)> {
    let bytes = args.as_bytes();
    let mut depth = 0i32;
    let mut in_string: Option<u8> = None;
    for (i, &b) in bytes.iter().enumerate() {
        if let Some(quote) = in_string {
            if b == b'\\' {
                continue;
            }
            if b == quote {
                in_string = None;
            }
            continue;
        }
        match b {
            b'"' | b'\'' => in_string = Some(b),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => return Some((&args[..i], &args[i + 1..])),
            _ => {}
        }
    }
    None
}

/// Translate the high-level `cat` (CAT / string-concat) operator to C#'s
/// `+`. The replacement only fires for ` cat ` tokens that sit outside
/// string literals so contents like `"a cat b"` are preserved verbatim.
fn rewrite_cat_operator(line: &str) -> String {
    if !line.contains(" cat ") {
        return line.to_string();
    }
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    let mut in_string: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(quote) = in_string {
            out.push(b as char);
            if b == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
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
        if b == b' '
            && i + 4 < bytes.len()
            && &bytes[i..i + 5] == b" cat "
            // Only treat ` cat ` as the operator when both flanks are
            // already part of expression context (something on the
            // left). At i==0 we'd have `cat ` at the start of a line,
            // which is more likely to be an identifier — leave alone.
            && i > 0
        {
            out.push_str(" + ");
            i += 5;
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    out
}

/// Returns true if `line` already terminates control flow such that a trailing
/// `break;` would be unreachable. Used by C# switch-case rendering to skip
/// inserting a redundant `break;` after `return`/`throw`/`goto`/`break`.
pub(in crate::decompiler) fn line_is_csharp_terminator(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("return ")
        || trimmed == "return;"
        || trimmed.starts_with("return;")
        || trimmed.starts_with("throw ")
        || trimmed == "throw;"
        || trimmed.starts_with("goto ")
        || trimmed == "break;"
        || trimmed == "continue;"
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

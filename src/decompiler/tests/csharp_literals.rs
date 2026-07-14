use super::super::*;
use super::*;

#[test]
fn legacy_statement_to_csharp_does_not_panic_on_degenerate_headers() {
    // Regression: empty-condition `if {` / `while {` headers used raw byte
    // slicing (`trimmed[3..len-2]`) which panicked with begin > end. They must
    // be handled without panicking.
    let _ = legacy_statement_to_csharp("if {");
    let _ = legacy_statement_to_csharp("while {");
    // Regression (adversarial): a multibyte UTF-8 character outside a string
    // literal (e.g. a method-token name) previously panicked in the helper
    // rewriter via `&line[i..]` landing mid-character. It must be preserved.
    assert_eq!(legacy_statement_to_csharp("é(t0);"), "é(t0);");
    assert_eq!(
        legacy_statement_to_csharp("let t0 = \"café\";"),
        "var t0 = \"café\";"
    );
    let _ = legacy_statement_to_csharp("naïve(abs(x));");
    // Well-formed headers still convert.
    assert_eq!(
        legacy_statement_to_csharp("if loc0 == 1 {"),
        "if (loc0 == 1) {"
    );
    assert_eq!(
        legacy_statement_to_csharp("while loc0 < 3 {"),
        "while (loc0 < 3) {"
    );
}

#[test]
fn csharpize_nested_helper_calls_in_cast_path_helpers() {
    // pow/left/right/substr take the int-cast argument path; nested NEO helper
    // calls in their arguments must still be rewritten to compilable C# rather
    // than emitted verbatim.
    assert_eq!(
        legacy_statement_to_csharp("let t0 = pow(abs(x), 2);"),
        "var t0 = BigInteger.Pow(BigInteger.Abs(x), 2);"
    );
}

#[test]
fn legacy_expression_to_csharp_preserves_multibyte_in_cat_path() {
    // Regression (adversarial recheck): rewrite_cat_operator runs first in
    // legacy_expression_to_csharp and previously mangled multibyte UTF-8 (push b as char
    // re-encodes as Latin-1) when the line contained ` cat `.
    assert_eq!(
        legacy_statement_to_csharp("let t0 = café cat x;"),
        "var t0 = café + x;"
    );
    assert_eq!(
        legacy_statement_to_csharp("let t0 = \"naïve\" cat y;"),
        "var t0 = \"naïve\" + y;"
    );
}

#[test]
fn csharp_escapes_control_chars_in_pushdata_string_literal() {
    // PUSHDATA1 "a\nb" (a, raw newline, b) + RET. A raw newline inside a C#
    // string constant is error CS1010, so the lifted literal must escape it.
    let nef_bytes = build_nef(&[0x0C, 0x03, b'a', b'\n', b'b', 0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");
    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains(r#""a\nb""#),
        "newline in PUSHDATA string must be escaped as \\n: {csharp}"
    );
    assert!(
        !csharp.contains("a\nb"),
        "a raw newline must not appear inside the C# string literal: {csharp}"
    );
}

#[test]
fn csharp_non_void_method_with_empty_body_throws_not_implemented() {
    // `prologue` spans [0,3) = INITSLOT only, which lifts to no statements.
    // A non-void (Integer) return with no body is C# error CS0161, so the
    // renderer must emit a throwing stub rather than a bare comment.
    let nef_bytes = build_nef(&[0x57, 0x00, 0x00, 0x11, 0x12, 0x9E, 0x40, 0x20, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"{"name":"Demo","abi":{"methods":[
            {"name":"prologue","returntype":"Integer","offset":0,"parameters":[],"safe":false},
            {"name":"body","returntype":"Integer","offset":3,"parameters":[],"safe":false}
        ],"events":[]}}"#,
    )
    .expect("manifest parsed");
    // Clean mode (trace comments off) is the user-facing compilable C# output —
    // the mode the CLI `decompile --format csharp` emits. There INITSLOT lifts
    // to no statements, so `prologue`'s body is empty.
    let decompilation = Decompiler::new()
        .with_trace_comments(false)
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");
    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("BigInteger prologue()"),
        "prologue should render with its declared return type: {csharp}"
    );
    assert!(
        csharp.contains("throw new NotImplementedException();"),
        "non-void method with an empty lifted body must throw, not emit a bare comment: {csharp}"
    );
}

#[test]
fn csharp_void_event_parameter_renders_as_object_not_void() {
    // An event arg typed `Void` previously rendered `Action<void>`
    // (C# error CS1547: void cannot be a type argument). It must map to
    // `object` in non-return position.
    let nef_bytes = build_nef(&[0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"{"name":"Ev","abi":{"methods":[
            {"name":"main","returntype":"Void","offset":0,"parameters":[],"safe":false}
        ],"events":[{"name":"Boom","parameters":[{"name":"x","type":"Void"}]}]}}"#,
    )
    .expect("manifest parsed");
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");
    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("Action<object> Boom"),
        "void event parameter must render as object: {csharp}"
    );
    assert!(
        !csharp.contains("Action<void>"),
        "void must never appear as a generic type argument: {csharp}"
    );
}

#[test]
fn csharp_wraps_only_oversized_integer_literals() {
    // PUSHINT256 = 1<<200 (a 61-digit value, > ulong.MaxValue) + RET. A bare
    // decimal literal above ulong.MaxValue is C# error CS1021, so it must
    // become BigInteger.Parse("…").
    let mut script = vec![0x05];
    let mut operand = vec![0u8; 32];
    operand[25] = 0x01; // bit 200
    script.extend_from_slice(&operand);
    script.push(0x40);
    let nef_bytes = build_nef(&script);
    let csharp = Decompiler::new()
        .with_inline_single_use_temps(true)
        .with_trace_comments(false)
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds")
        .csharp
        .expect("csharp output");
    assert!(
        csharp.contains(
            r#"BigInteger.Parse("1606938044258990275541962092341162602522202993782792835301376")"#
        ),
        "oversized integer literal must be wrapped in BigInteger.Parse: {csharp}"
    );

    // Boundary + non-decimal cases via the statement rewriter:
    // u64::MAX fits in a C# `ulong` literal — leave it bare.
    assert_eq!(
        legacy_statement_to_csharp("return 18446744073709551615;"),
        "return 18446744073709551615;"
    );
    // u64::MAX + 1 exceeds `ulong` — wrap it.
    assert_eq!(
        legacy_statement_to_csharp("return 18446744073709551616;"),
        r#"return BigInteger.Parse("18446744073709551616");"#
    );
    // Small literals, hex (syscall hashes), and label identifiers are untouched.
    assert_eq!(legacy_statement_to_csharp("return 42;"), "return 42;");
    assert!(
        !legacy_statement_to_csharp("let t0 = syscall(0xDEADBEEF);").contains("BigInteger.Parse")
    );
    assert!(!legacy_statement_to_csharp("goto label_0x0010;").contains("BigInteger.Parse"));
}

#[test]
fn csharp_renders_oversized_hex_blob_as_byte_array() {
    // PUSHDATA1 of 20 non-printable bytes (0xA0..0xB3) + RET. The lift renders
    // a non-printable blob as `0x<HEX>`; a >16-digit hex value is C# error
    // CS1021 as an integer, so it must become a byte[] literal.
    let blob: Vec<u8> = (0..20).map(|i| 0xA0 + i).collect();
    let mut script = vec![0x0C, 20];
    script.extend_from_slice(&blob);
    script.push(0x40);
    let nef_bytes = build_nef(&script);
    let csharp = Decompiler::new()
        .with_inline_single_use_temps(true)
        .with_trace_comments(false)
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds")
        .csharp
        .expect("csharp output");
    assert!(
        csharp.contains("new byte[] { 0xA0, 0xA1,") && csharp.contains("0xB3 }"),
        "wide hex blob must render as a byte[] literal: {csharp}"
    );

    // Short hex (syscall hashes, CALLT indices, labels) must NOT be touched.
    assert!(!legacy_statement_to_csharp("let t0 = syscall(0xEFBEADDE);").contains("byte[]"));
    assert!(!legacy_statement_to_csharp("let t0 = callt(0x0000);").contains("byte[]"));
    assert!(!legacy_statement_to_csharp("goto label_0x0010;").contains("byte[]"));
}

#[test]
fn csharp_renders_map_literal_as_collection_initializer() {
    // PUSH4 PUSH3 PUSH2 PUSH1 PUSH2(count) PACKMAP RET keeps source order as
    // Map(1: 2, 3: 4). The
    // `Map(k: v)` form's `:` is invalid in a C# call (CS1026); it must render
    // as a collection initializer.
    let script = [0x14, 0x13, 0x12, 0x11, 0x12, 0xBE, 0x40];
    let nef_bytes = build_nef(&script);
    let csharp = Decompiler::new()
        .with_inline_single_use_temps(true)
        .with_trace_comments(false)
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds")
        .csharp
        .expect("csharp output");
    assert!(
        csharp.contains("new Map<object, object> { [1] = 2, [3] = 4 }"),
        "non-empty map literal must render as a C# collection initializer: {csharp}"
    );
    assert!(
        !csharp.contains("Map(1: 2"),
        "the invalid `Map(k: v)` form must not appear: {csharp}"
    );

    // The empty map keeps the constructor form, and the statement rewriter
    // leaves a non-map `Map(...)` (none exists in the lift) untouched.
    assert_eq!(
        legacy_statement_to_csharp("let t0 = Map();"),
        "var t0 = new Map<object, object>();"
    );
}

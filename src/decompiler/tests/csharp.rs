use super::*;

#[test]
fn csharp_synthetic_script_entry_exposes_initslot_args_and_preserves_return() {
    // Bytecode: INITSLOT 0,1; LDARG0; ISNULL; NOT; RET — declares one
    // argument and returns its non-null-ness. Without a manifest, the
    // C# emitter must (a) surface the arg as a parameter so the body's
    // `arg0` reference resolves, and (b) preserve the lifted return
    // value (the previous hardcoded `void` signature silently dropped
    // it).
    let nef_bytes = build_nef(&[0x57, 0x00, 0x01, 0x78, 0xD8, 0xAA, 0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("public static object ScriptEntry(object arg0)"),
        "synthetic ScriptEntry should declare INITSLOT-counted args: {csharp}"
    );
    // Verbose-mode (lib API default) keeps the temp; clean-mode would
    // inline. Either form preserves the return value, which is the
    // bug we care about — the previous `void` signature dropped it.
    assert!(
        csharp.contains("return !t0;") || csharp.contains("return !(arg0 is null);"),
        "lifted return value should be preserved (not dropped via void signature): {csharp}"
    );
    // Also verify the body actually references arg0 (not just the
    // signature) so the param isn't unused boilerplate.
    assert!(
        csharp.contains("(arg0 is null)") || csharp.contains("is_null(arg0)"),
        "body should reference arg0 from the new parameter: {csharp}"
    );
}

#[test]
fn csharp_omits_trailing_return_in_void_methods() {
    // Smallest script: a single RET. Without a manifest the synthetic
    // ScriptEntry now defaults to `object` return (so any pushed value
    // is preserved instead of dropped) — for a bare RET that produces
    // `return default;` in the body. The historical behavior (`void`
    // signature with `return;` stripped) was buggy for non-void scripts
    // (it silently discarded the lifted return value).
    let nef_bytes = build_nef(&[0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("public static object ScriptEntry()"),
        "synthetic ScriptEntry should default to object return when no manifest is provided: {csharp}"
    );
    // Bare RET on empty stack lifts to `return;`. With a non-void
    // signature the C# render rewrites that to `return default;` to
    // satisfy the type system.
    assert!(
        csharp.contains("return default;"),
        "synthetic ScriptEntry with bare RET should yield `return default;`: {csharp}"
    );
}

#[test]
fn csharp_keeps_explicit_return_value_in_non_void_methods() {
    // Script: PUSH1 RET — high-level lifts as `return 1;`, which the C#
    // emitter must preserve since the method is non-void.
    let nef_bytes = build_nef(&[0x11, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "ReturnsInt",
                "abi": {
                    "methods": [
                        {
                            "name": "main",
                            "parameters": [],
                            "returntype": "Integer",
                            "offset": 0
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*"
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("return 1;"),
        "non-void method should keep its computed return: {csharp}"
    );
}

#[test]
fn csharp_translates_loop_to_while_true() {
    // Script: INITSLOT; PUSH0; STLOC0; (loop top:) LDLOC0; PUSH3; LT;
    // JMPIFNOT to JMP; NOP; LDLOC0; PUSH1; ADD; STLOC0; JMP back-to-PUSH0; RET.
    // The JMP here targets the `STLOC0` initialization (an infinite reset
    // loop), so the high-level post-pass collapses the `label: ... goto label;`
    // pattern into `loop { ... }`. The C# emitter must rewrite that into a
    // valid C# `while (true)`.
    let script = [
        0x57, 0x01, 0x00, 0x10, 0x70, 0x68, 0x13, 0xB5, 0x26, 0x07, 0x21, 0x68, 0x11, 0x9E, 0x70,
        0x22, 0xF4, 0x40,
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("while (true) {"),
        "C# output should translate `loop {{` to `while (true) {{`: {csharp}"
    );
    assert!(
        !csharp.contains("loop {"),
        "C# output should not retain the high-level `loop` keyword: {csharp}"
    );
}

#[test]
fn csharp_translates_switch_to_idiomatic_c_sharp() {
    // Script: INITSLOT; STLOC0(1); equality chain on loc0 with cases 0,1,default
    let script = [
        0x57, 0x01, 0x00, 0x11, 0x70, 0x68, 0x10, 0x97, 0x26, 0x06, 0x1A, 0x70, 0x22, 0x0D, 0x68,
        0x11, 0x97, 0x26, 0x06, 0x1B, 0x70, 0x22, 0x04, 0x1C, 0x70, 0x68, 0x40,
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("switch (loc0) {"),
        "switch scrutinee should be parenthesised: {csharp}"
    );
    assert!(
        csharp.contains("case 0: {"),
        "case label should use C# `: {{` form: {csharp}"
    );
    assert!(
        csharp.contains("case 1: {"),
        "second case should also use `: {{`: {csharp}"
    );
    assert!(
        csharp.contains("default: {"),
        "default label should use `default: {{`: {csharp}"
    );
    // Each case body must end in a control-transfer statement; with the
    // simple PUSH/STLOC bodies here the emitter inserts `break;` before the
    // matching close brace so the switch compiles under C#.
    let break_count = csharp.matches("break;").count();
    assert!(
        break_count >= 3,
        "each case (including default) should end with `break;` (found {break_count}): {csharp}"
    );
    assert!(
        !csharp.contains("case 0 {"),
        "C# output should not retain the high-level `case X {{` form: {csharp}"
    );
    assert!(
        !csharp.contains("default {"),
        "C# output should not retain the high-level `default {{` form: {csharp}"
    );
}

#[test]
fn csharp_else_if_chain_uses_parenthesised_conditions() {
    // Same script as switch test — high-level emitter may or may not promote
    // it to a switch depending on heuristics; either way, any surviving
    // `else if` chain must be parenthesised in C#.
    let script = [
        0x57, 0x01, 0x00, 0x11, 0x70, 0x68, 0x10, 0x97, 0x26, 0x06, 0x1A, 0x70, 0x22, 0x0D, 0x68,
        0x11, 0x97, 0x26, 0x06, 0x1B, 0x70, 0x22, 0x04, 0x1C, 0x70, 0x68, 0x40,
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    // No bare `else if X {` (without parens) should survive.
    for line in csharp.lines() {
        let trimmed = line.trim();
        assert!(
            !trimmed.starts_with("else if ") || trimmed.starts_with("else if ("),
            "C# else-if must use parenthesised condition: {trimmed}"
        );
        assert!(
            !trimmed.starts_with("} else if ") || trimmed.starts_with("} else if ("),
            "C# `}} else if` must use parenthesised condition: {trimmed}"
        );
    }
}

#[test]
fn csharp_view_respects_manifest_metadata_and_parameters() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "Demo",
                "abi": {
                    "methods": [
                        {
                            "name": "deploy-contract",
                            "parameters": [
                                {"name": "owner-name", "type": "Hash160"},
                                {"name": "amount", "type": "Integer"}
                            ],
                            "returntype": "Void",
                            "offset": 0
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*",
                "extra": {"Author": "Jane Doe", "Email": "jane@example.com"}
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(csharp.contains("[ManifestExtra(\"Author\", \"Jane Doe\")]"));
    assert!(csharp.contains("[ManifestExtra(\"Email\", \"jane@example.com\")]"));
    assert!(csharp
        .contains("public static void deploy_contract(UInt160 owner_name, BigInteger amount)"));
}

#[test]
fn high_level_view_renders_manifest_groups_block() {
    // `groups` (signed pubkey memberships authorising contract
    // updates) was dropped from the high-level summary. The C#
    // emitter has no idiomatic place to surface this metadata since
    // it's set at deployment time, not declared in source — but the
    // high-level summary is meant to be a complete inspection view of
    // the manifest, so it should show what the manifest contains.
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "Demo",
                "groups": [
                    {"pubkey": "02f49ce0c33aabbccdd", "signature": "BAt..."},
                    {"pubkey": "02b00b1eaaaabbbbcccc", "signature": "BAd..."}
                ],
                "abi": {"methods": [], "events": []},
                "permissions": [],
                "trusts": "*",
                "extra": {}
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("groups {"),
        "high-level should open a groups block:\n{high_level}"
    );
    assert!(high_level.contains("pubkey=02f49ce0c33aabbccdd"));
    assert!(high_level.contains("pubkey=02b00b1eaaaabbbbcccc"));
    // Signature is intentionally elided — opaque base64, no human value.
    assert!(!high_level.contains("BAt..."));
    assert!(!high_level.contains("signature="));

    // C# header should mirror the high-level rendering with a
    // `// groups:` comment block (parity with the existing
    // `// permissions:` and `// trusts:` blocks). The `groups` field
    // has no source-level attribute in Neo SmartContract Framework
    // (set at deployment, not declared in code), so a comment is the
    // right surface for completeness.
    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("// groups:"),
        "C# header should open a groups comment block:\n{csharp}"
    );
    assert!(csharp.contains("//   pubkey=02f49ce0c33aabbccdd"));
    assert!(csharp.contains("//   pubkey=02b00b1eaaaabbbbcccc"));
    // Same elision policy as high-level: signature is opaque.
    assert!(!csharp.contains("BAt..."));
    assert!(!csharp.contains("signature="));
}

#[test]
fn csharp_view_renders_non_string_scalar_extra_metadata() {
    // Manifests in the wild occasionally carry numeric or boolean
    // entries in `extra` (e.g. `"Version": 1`, `"Verified": true`).
    // The renderer used to gate on `value.as_str()` and silently drop
    // anything else, hiding real metadata. Now both renderers
    // stringify scalars via `render_extra_scalar`, so users see the
    // value verbatim.
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "Demo",
                "abi": {"methods": [], "events": []},
                "permissions": [],
                "trusts": "*",
                "extra": {
                    "Author": "Anon",
                    "Version": 2,
                    "Verified": true,
                    "Notes": null
                }
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(csharp.contains("[ManifestExtra(\"Author\", \"Anon\")]"));
    assert!(csharp.contains("[ManifestExtra(\"Version\", \"2\")]"));
    assert!(csharp.contains("[ManifestExtra(\"Verified\", \"true\")]"));
    // null has no canonical short form — entry is dropped, not rendered as "null".
    assert!(!csharp.contains("ManifestExtra(\"Notes\""));

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(high_level.contains("// Author: Anon"));
    assert!(high_level.contains("// Version: 2"));
    assert!(high_level.contains("// Verified: true"));
    assert!(!high_level.contains("// Notes:"));
}

#[test]
fn csharpize_statement_converts_known_forms() {
    assert_eq!(csharpize_statement("   "), "");
    assert_eq!(csharpize_statement("// note"), "// note");
    assert_eq!(csharpize_statement("let x = 1;"), "var x = 1;");
    // Helper rewrites must apply inside `let` initialisers too —
    // earlier the `let` branch early-returned before
    // `csharpize_expression` ran, so `let t0 = min(x, y);` came
    // out as `var t0 = min(x, y);` (uncompilable).
    assert_eq!(
        csharpize_statement("let t0 = min(x, y);"),
        "var t0 = BigInteger.Min(x, y);"
    );
    assert_eq!(
        csharpize_statement("let t0 = is_null(loc0);"),
        "var t0 = (loc0 is null);"
    );
    assert_eq!(csharpize_statement("let t0 = a cat b;"), "var t0 = a + b;");
    // Helper rewrites must also apply inside throw / abort / assert
    // operands. Same bug class as the `let` branch fix — these
    // branches were extracting their bodies but not running them
    // through the expression rewriter.
    // Non-string operands get wrapped in `$"{...}"` so the
    // implicit ToString call satisfies `Exception(string)`. Without
    // this, `new Exception(BigInteger.Min(a, b))` had no matching
    // constructor.
    assert_eq!(
        csharpize_statement("throw(min(a, b));"),
        "throw new Exception($\"{BigInteger.Min(a, b)}\");"
    );
    // String-concat results pass through unwrapped — any `"…"`
    // somewhere in the operand signals a string-typed result, so
    // wrapping in `$"{...}"` would be redundant noise. The
    // resulting `Exception("err" + code)` compiles because `+` on
    // a `string` left operand always produces a `string`.
    assert_eq!(
        csharpize_statement("abort(\"err\" cat code);"),
        "throw new Exception(\"err\" + code);"
    );
    assert_eq!(
        csharpize_statement("assert(is_null(loc0));"),
        "if (!((loc0 is null))) throw new Exception();"
    );
    // Assert message is also coerced to satisfy `Exception(string)`:
    // a string-concat result passes through unwrapped (it has a `"`
    // somewhere), while a non-string message gets the same
    // `$"{value}"` wrap as throw/abort.
    assert_eq!(
        csharpize_statement("assert(min(a, b) > 0, \"e\" cat code);"),
        "if (!(BigInteger.Min(a, b) > 0)) throw new Exception(\"e\" + code);"
    );
    assert_eq!(
        csharpize_statement("assert(x > 0, code);"),
        "if (!(x > 0)) throw new Exception($\"{code}\");"
    );
    assert_eq!(csharpize_statement("if t0 {"), "if (t0) {");
    assert_eq!(csharpize_statement("while t1 {"), "while (t1) {");
    assert_eq!(csharpize_statement("loop {"), "while (true) {");
    assert_eq!(
        csharpize_statement("else if loc0 < 3 {"),
        "else if (loc0 < 3) {"
    );
    assert_eq!(
        csharpize_statement("} else if loc0 == 1 {"),
        "} else if (loc0 == 1) {"
    );
    assert_eq!(
        csharpize_statement("for (let i = 0; i < 3; i++) {"),
        "for (var i = 0; i < 3; i++) {"
    );
    assert_eq!(
        csharpize_statement("leave label_0x0010;"),
        "goto label_0x0010;"
    );
    // CAT operator (high-level pseudocode) → C# `+`. The translation
    // only fires for ` cat ` tokens outside string literals.
    assert_eq!(
        csharpize_statement("return \"b:\" cat addr;"),
        "return \"b:\" + addr;"
    );
    assert_eq!(
        csharpize_statement("var x = a cat b cat c;"),
        "var x = a + b + c;"
    );
    assert_eq!(
        csharpize_statement("var msg = \"says cat ok\";"),
        "var msg = \"says cat ok\";"
    );
    // `throw(value);` (high-level pseudocode for NEO's THROW opcode)
    // becomes `throw new Exception(value);` in C# — NEO accepts any
    // stack value, but C# requires an `Exception`.
    assert_eq!(
        csharpize_statement("throw(\"oops\");"),
        "throw new Exception(\"oops\");"
    );
    // Non-string-literal identifier — wrapped to coerce via
    // ToString. The user can hand-strip `$"{...}"` if they know
    // `error_msg` is already a string.
    assert_eq!(
        csharpize_statement("throw(error_msg);"),
        "throw new Exception($\"{error_msg}\");"
    );
    // ABORT / ABORTMSG also map to `throw new Exception(...);` —
    // closest C# analogue to a NEO VM abort.
    assert_eq!(csharpize_statement("abort();"), "throw new Exception();");
    assert_eq!(
        csharpize_statement("abort(\"fatal\");"),
        "throw new Exception(\"fatal\");"
    );
    // Identifier operand — same wrapping rule as `throw(error_msg)`
    // since we don't know its static type.
    assert_eq!(
        csharpize_statement("abort(reason);"),
        "throw new Exception($\"{reason}\");"
    );
    // ASSERT / ASSERTMSG also need a compilable C# form. There is no
    // built-in `assert` keyword/function; the universal translation
    // is `if (!(cond)) throw new Exception(...);`.
    assert_eq!(
        csharpize_statement("assert(x > 0);"),
        "if (!(x > 0)) throw new Exception();"
    );
    assert_eq!(
        csharpize_statement("assert(x > 0, \"must be positive\");"),
        "if (!(x > 0)) throw new Exception(\"must be positive\");"
    );
    // Don't be fooled by commas inside the condition expression.
    assert_eq!(
        csharpize_statement("assert(foo(a, b));"),
        "if (!(foo(a, b))) throw new Exception();"
    );
    // NEO arithmetic helpers — the high-level lift emits `abs/min/max/pow`
    // as bare function calls, but C# has no `abs` etc. in scope. Rewrite
    // to `BigInteger.X(...)`. For `pow`, the second argument must be
    // `int` per `BigInteger.Pow`'s signature.
    assert_eq!(
        csharpize_statement("var x = abs(loc0);"),
        "var x = BigInteger.Abs(loc0);"
    );
    assert_eq!(
        csharpize_statement("var x = min(a, b);"),
        "var x = BigInteger.Min(a, b);"
    );
    assert_eq!(
        csharpize_statement("var x = max(a, b);"),
        "var x = BigInteger.Max(a, b);"
    );
    assert_eq!(
        csharpize_statement("var x = pow(base, exp);"),
        "var x = BigInteger.Pow(base, (int)(exp));"
    );
    // Literal exponent skips the redundant `(int)` cast — same idea
    // as `wrap_int_cast_unless_literal`. `pow(2, 8)` lifts cleanly to
    // `BigInteger.Pow(2, 8)` rather than `BigInteger.Pow(2, (int)(8))`.
    assert_eq!(
        csharpize_statement("var x = pow(2, 8);"),
        "var x = BigInteger.Pow(2, 8);"
    );
    assert_eq!(
        csharpize_statement("var x = left(buf, 4);"),
        "var x = Helper.Left(buf, 4);"
    );
    assert_eq!(
        csharpize_statement("var x = substr(buf, 0, 16);"),
        "var x = Helper.Substr(buf, 0, 16);"
    );
    // Identifier-boundary respect: `mypow(x)` is NOT `pow(x)`.
    assert_eq!(
        csharpize_statement("var x = mypow(2);"),
        "var x = mypow(2);"
    );
    // String-literal preservation: `"min(a)"` inside a string stays
    // verbatim.
    assert_eq!(
        csharpize_statement("var x = \"min(a, b)\";"),
        "var x = \"min(a, b)\";"
    );
    // Nested helpers compose: `max(abs(a), b)` → `BigInteger.Max(BigInteger.Abs(a), b)`.
    assert_eq!(
        csharpize_statement("var x = max(abs(a), b);"),
        "var x = BigInteger.Max(BigInteger.Abs(a), b);"
    );
    // Extended NEO arithmetic / buffer helpers — `BigInteger.X` for
    // ones .NET provides directly, `Helper.X` (Neo SmartContract
    // Framework) for the rest. Args at int-typed positions get an
    // `(int)(...)` cast so the C# overload signature matches.
    assert_eq!(
        csharpize_statement("var x = sign(loc0);"),
        "var x = Helper.Sign(loc0);"
    );
    assert_eq!(
        csharpize_statement("var x = sqrt(loc0);"),
        "var x = Helper.Sqrt(loc0);"
    );
    assert_eq!(
        csharpize_statement("var x = modmul(a, b, m);"),
        "var x = Helper.ModMul(a, b, m);"
    );
    assert_eq!(
        csharpize_statement("var x = modpow(b, e, m);"),
        "var x = BigInteger.ModPow(b, e, m);"
    );
    assert_eq!(
        csharpize_statement("var x = within(v, lo, hi);"),
        "var x = Helper.Within(v, lo, hi);"
    );
    assert_eq!(
        csharpize_statement("var x = left(buf, n);"),
        "var x = Helper.Left(buf, (int)(n));"
    );
    assert_eq!(
        csharpize_statement("var x = right(buf, n);"),
        "var x = Helper.Right(buf, (int)(n));"
    );
    assert_eq!(
        csharpize_statement("var x = substr(buf, start, len);"),
        "var x = Helper.Substr(buf, (int)(start), (int)(len));"
    );
    // `is_null(x)` is a unary check, not a function call — it lifts
    // to the idiomatic C# pattern `(x is null)` instead of trying to
    // resolve a (non-existent) `IsNull` helper on the framework.
    assert_eq!(
        csharpize_statement("if is_null(loc0) {"),
        "if ((loc0 is null)) {"
    );
    assert_eq!(
        csharpize_statement("var x = is_null(loc0);"),
        "var x = (loc0 is null);"
    );
    // Nested into another helper: `if (!is_null(x))` style usages.
    assert_eq!(
        csharpize_statement("var y = !is_null(loc0);"),
        "var y = !(loc0 is null);"
    );
    // Identifier-boundary respect: `assert_is_null(x)` must NOT pick
    // up the `is_null` rewrite (it's a different identifier).
    assert_eq!(
        csharpize_statement("var x = my_is_null(loc0);"),
        "var x = my_is_null(loc0);"
    );
    // Empty collection constructors lifted from NEWMAP / NEWARRAY0 /
    // NEWSTRUCT0 — the lift emits `Map()`, `[]`, `Struct()` which
    // don't compile as-is. Rewrite to explicit `new` forms with
    // best-effort type defaults (`object` for Map's generic args
    // since we don't have key/value type info; `object[0]` for the
    // bare-literal array case).
    assert_eq!(
        csharpize_statement("var t0 = Map();"),
        "var t0 = new Map<object, object>();"
    );
    assert_eq!(
        csharpize_statement("var t0 = [];"),
        "var t0 = new object[0];"
    );
    assert_eq!(
        csharpize_statement("var t0 = Struct();"),
        "var t0 = new Struct();"
    );
    // Identifier-boundary respect — a user-named `MyMap()` factory
    // must NOT be rewritten to `new MyMap<...>()`.
    assert_eq!(
        csharpize_statement("var t0 = MyMap();"),
        "var t0 = MyMap();"
    );
    // String-literal preservation — `"Map()"` inside a quoted
    // string stays verbatim.
    assert_eq!(
        csharpize_statement("var t0 = \"Map()\";"),
        "var t0 = \"Map()\";"
    );
    // Size-operand constructors lifted from NEWBUFFER / NEWARRAY
    // — `new_buffer(n)` and `new_array(n)` aren't valid C#
    // identifiers; rewrite to explicit `new byte[...]` /
    // `new object[...]`. The size operand needs a defensive
    // `(int)` cast for any expression that could carry BigInteger
    // semantics, but bare integer literals are unambiguously `int`
    // to the C# parser, so `wrap_int_cast_unless_literal` skips the
    // cast for them — yielding `new object[3]` instead of the noisier
    // `new object[(int)(3)]`. Variable / expression operands still
    // get the cast.
    assert_eq!(
        csharpize_statement("var t0 = new_buffer(8);"),
        "var t0 = new byte[8];"
    );
    assert_eq!(
        csharpize_statement("var t0 = new_buffer(loc0);"),
        "var t0 = new byte[(int)(loc0)];"
    );
    assert_eq!(
        csharpize_statement("var t0 = new_array(3);"),
        "var t0 = new object[3];"
    );
    // Negative literals also pass through without cast. Negative
    // sizes don't make sense for `new T[]` but the cast wouldn't
    // help anyway — `new T[-3]` and `new T[(int)(-3)]` are both
    // accepted by the C# compiler and reject at runtime alike.
    assert_eq!(
        csharpize_statement("var t0 = new_array(-3);"),
        "var t0 = new object[-3];"
    );
    // Identifier-boundary respect — `my_new_buffer(8)` is NOT the
    // NEWBUFFER lift output.
    assert_eq!(
        csharpize_statement("var t0 = my_new_buffer(8);"),
        "var t0 = my_new_buffer(8);"
    );
    // CONVERT lifts (`convert_to_bool` / `convert_to_integer` /
    // `convert_to_bytestring` / `convert_to_buffer`) — rewrite to
    // explicit C# casts.
    assert_eq!(
        csharpize_statement("var t0 = convert_to_bool(loc0);"),
        "var t0 = (bool)(loc0);"
    );
    assert_eq!(
        csharpize_statement("var t0 = convert_to_integer(loc0);"),
        "var t0 = (BigInteger)(loc0);"
    );
    assert_eq!(
        csharpize_statement("var t0 = convert_to_bytestring(loc0);"),
        "var t0 = (ByteString)(loc0);"
    );
    assert_eq!(
        csharpize_statement("var t0 = convert_to_buffer(loc0);"),
        "var t0 = (byte[])(loc0);"
    );
    // ISTYPE lifts — rewrite to C# pattern matches.
    assert_eq!(
        csharpize_statement("if is_type_bool(loc0) {"),
        "if ((loc0 is bool)) {"
    );
    assert_eq!(
        csharpize_statement("var t0 = is_type_integer(loc0);"),
        "var t0 = (loc0 is BigInteger);"
    );
    assert_eq!(
        csharpize_statement("var t0 = is_type_bytestring(loc0);"),
        "var t0 = (loc0 is ByteString);"
    );
    assert_eq!(
        csharpize_statement("var t0 = is_type_buffer(loc0);"),
        "var t0 = (loc0 is byte[]);"
    );
    // The other CONVERT / ISTYPE variants (any, pointer, array,
    // struct, map, interopinterface) deliberately keep the lifted
    // form — silently rewriting them would require type info the
    // lift doesn't supply. Leave a clear hint that the user has to
    // pick the right cast.
    assert_eq!(
        csharpize_statement("var t0 = convert_to_array(loc0);"),
        "var t0 = convert_to_array(loc0);"
    );
    assert_eq!(
        csharpize_statement("var t0 = is_type_map(loc0);"),
        "var t0 = is_type_map(loc0);"
    );
    // Collection helpers — `clear_items(c)`, `remove_item(c, k)`,
    // `keys(m)`, `values(m)`, `reverse_items(arr)` are NEO-flavoured
    // identifiers that don't compile. Rewrite to standard
    // .NET / Neo Map accessors.
    assert_eq!(csharpize_statement("clear_items(loc0);"), "loc0.Clear();");
    assert_eq!(
        csharpize_statement("remove_item(loc0, key);"),
        "loc0.Remove(key);"
    );
    assert_eq!(
        csharpize_statement("var t0 = keys(loc0);"),
        "var t0 = loc0.Keys;"
    );
    assert_eq!(
        csharpize_statement("var t0 = values(loc0);"),
        "var t0 = loc0.Values;"
    );
    assert_eq!(
        csharpize_statement("reverse_items(loc0);"),
        "loc0.Reverse();"
    );
    // Identifier-boundary respect — `my_keys(loc0)` is NOT KEYS.
    assert_eq!(
        csharpize_statement("var t0 = my_keys(loc0);"),
        "var t0 = my_keys(loc0);"
    );
    // APPEND lift — `append(arr, item)` → `arr.Add(item)`.
    assert_eq!(csharpize_statement("append(loc0, 42);"), "loc0.Add(42);");
    // HASKEY lift — `has_key(c, k)` → `c.ContainsKey(k)`.
    assert_eq!(
        csharpize_statement("var t0 = has_key(loc0, key);"),
        "var t0 = loc0.ContainsKey(key);"
    );
    assert_eq!(
        csharpize_statement("if has_key(loc0, key) {"),
        "if (loc0.ContainsKey(key)) {"
    );
}

#[test]
fn csharp_resolves_internal_calls_to_method_names() {
    // Script layout:
    // 0x0000: CALL +4 (target=0x0004)
    // 0x0002: RET
    // 0x0003: NOP
    // 0x0004: RET
    let script = [0x34, 0x04, 0x40, 0x21, 0x40];
    let nef_bytes = build_nef(&script);

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("sub_0x0004()"),
        "C# output should resolve internal helper names instead of raw call placeholders: {csharp}"
    );
    assert!(
        !csharp.contains("call_0x0004"),
        "C# output should not emit raw call_0x placeholders when a helper name is known: {csharp}"
    );
}

#[test]
fn csharp_emits_inferred_helper_methods() {
    // Script layout:
    // 0x0000: CALL +4 (target=0x0004)
    // 0x0002: RET
    // 0x0003: NOP
    // 0x0004: RET
    let script = [0x34, 0x04, 0x40, 0x21, 0x40];
    let nef_bytes = build_nef(&script);

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("private static dynamic sub_0x0004()")
            || csharp.contains("private static void sub_0x0004()")
            || csharp.contains("private static BigInteger sub_0x0004()")
            || csharp.contains("private static object sub_0x0004()"),
        "C# output should emit inferred helper method definitions for resolved internal calls: {csharp}"
    );
    assert!(
        !csharp.contains("sub_0x0003"),
        "C# output should not emit nop-only inferred helper methods: {csharp}"
    );
}

#[test]
fn csharp_inferred_nonvoid_helpers_do_not_emit_bare_return() {
    // Script layout:
    // 0x0000: CALL +4 (target=0x0004)
    // 0x0002: RET
    // 0x0003: NOP
    // 0x0004: RET
    let script = [0x34, 0x04, 0x40, 0x21, 0x40];
    let nef_bytes = build_nef(&script);

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        !csharp.contains(
            "private static dynamic sub_0x0004()
        {
            // 0004: RET
            return;"
        ),
        "non-void inferred helper bodies should not emit bare return statements: {csharp}"
    );
}

#[test]
fn csharp_includes_offsetless_manifest_methods_as_stubs() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "Stubby",
                "abi": {
                    "methods": [
                        { "name": "main", "parameters": [], "returntype": "Void", "offset": 0 },
                        { "name": "helper", "parameters": [], "returntype": "Void" }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*"
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("public static void helper()"),
        "offsetless method should appear in C# skeleton"
    );
    assert!(
        csharp.contains("NotImplementedException"),
        "offsetless method should be emitted as a stub"
    );
}

#[test]
fn csharp_includes_manifest_events() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "Events",
                "abi": {
                    "methods": [
                        { "name": "main", "parameters": [], "returntype": "Void", "offset": 0 }
                    ],
                    "events": [
                        {
                            "name": "transfer-event",
                            "parameters": [
                                { "name": "from", "type": "Hash160" },
                                { "name": "to", "type": "Hash160" },
                                { "name": "amount", "type": "Integer" }
                            ]
                        }
                    ]
                },
                "permissions": [],
                "trusts": "*"
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(csharp.contains("[DisplayName(\"transfer-event\")]"));
    assert!(
        csharp.contains("public static event Action<UInt160, UInt160, BigInteger> transfer_event;")
    );
}

#[test]
fn csharp_escapes_reserved_keywords() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "class",
                "abi": {
                    "methods": [
                        {
                            "name": "class",
                            "parameters": [{ "name": "namespace", "type": "Integer" }],
                            "returntype": "Void",
                            "offset": 0
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*"
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(csharp.contains("public class @class : SmartContract"));
    assert!(csharp.contains("public static void @class(BigInteger @namespace)"));
}

#[test]
fn csharp_uses_label_style_for_transfer_placeholders() {
    // Script: ENDTRY +6 (jumps past intermediate code to the final RET),
    //         PUSH1, PUSH2, ADD, DROP, RET. The intermediate stack ops
    //         keep the `leave` from being a fallthrough so the lift
    //         exercises the label-style transfer path the C# emitter
    //         lowers to `goto label_X;`.
    let nef_bytes = build_nef(&[0x3D, 0x06, 0x11, 0x12, 0x9E, 0x75, 0x40]);

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");

    assert!(
        csharp.contains("goto label_0x0006;"),
        "C# should normalize leave-transfers to goto label style: {csharp}"
    );
    assert!(
        csharp.contains("label_0x0006:"),
        "C# should emit label declaration for transfer targets: {csharp}"
    );
    assert!(
        !csharp.contains("leave label_"),
        "C# should not emit non-C# leave statements: {csharp}"
    );
    assert!(
        !csharp.contains("leave_0x"),
        "C# should not emit legacy function-style transfer placeholders: {csharp}"
    );
}

#[test]
fn csharp_mismatch_offset_emits_script_entry_and_manifest_method() {
    // Script: PUSH1; RET; PUSH2; RET
    let nef_bytes = build_nef(&[0x11, 0x40, 0x12, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "OffsetMismatch",
                "abi": {
                    "methods": [
                        {
                            "name": "helper",
                            "parameters": [],
                            "returntype": "Integer",
                            "offset": 2
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*"
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    // Synthetic ScriptEntry now uses `object` return type — without
    // a matching manifest entry the emitter doesn't know whether
    // the script's RET is meant to discard or carry a value, so
    // `object` preserves what the bytecode pushed.
    assert!(
        csharp.contains("public static object ScriptEntry()"),
        "C# output should keep a synthetic script-entry method when ABI offsets do not include bytecode entry"
    );
    assert!(
        csharp.contains("public static BigInteger helper()"),
        "C# output should still emit the manifest method"
    );

    let before_helper = csharp
        .split("public static BigInteger helper")
        .next()
        .expect("entry section present");
    assert!(
        before_helper.contains("// 0000: PUSH1"),
        "script-entry body should contain bytecode from script start"
    );
    assert!(
        !before_helper.contains("// 0002: PUSH2"),
        "script-entry body should stop before helper method offset"
    );
}

#[test]
fn csharp_missing_manifest_offset_uses_first_method_as_entry_signature() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "OffsetMissing",
                "abi": {
                    "methods": [
                        {
                            "name": "main",
                            "parameters": [],
                            "returntype": "Integer"
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*"
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("public static BigInteger main()"),
        "C# output should reuse the first manifest method signature when offsets are missing"
    );
    assert!(
        !csharp.contains("public static void ScriptEntry()"),
        "synthetic ScriptEntry should not be emitted when the manifest omits entry offsets entirely"
    );
    assert!(
        !csharp.contains("NotImplementedException"),
        "the fallback entry method should not also be emitted as an offset-less stub"
    );
}

#[test]
fn csharp_trims_initslot_boundaries() {
    let Some(nef_bytes) = try_load_testing_nef("Contract_Delegate.nef") else {
        eprintln!("Skipping: Contract_Delegate.nef not found in devpack artifacts");
        return;
    };
    let Some(manifest) = try_load_testing_manifest("Contract_Delegate.manifest.json") else {
        eprintln!("Skipping: Contract_Delegate.manifest.json not found");
        return;
    };

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    let sum_block = csharp
        .split("public static BigInteger sumFunc")
        .nth(1)
        .and_then(|rest| rest.split("private static dynamic sub_0x000C").next())
        .expect("sumFunc block present");
    assert!(
        sum_block.contains("// 0000: INITSLOT"),
        "sumFunc should still show its entry INITSLOT"
    );
    assert!(
        !sum_block.contains("// 000C: INITSLOT"),
        "sumFunc body should stop before the inferred helper block"
    );
    assert!(
        !sum_block.contains("return t23;"),
        "duplicate return from appended block should not appear in sumFunc"
    );
    assert!(
        csharp.contains("private static dynamic sub_0x000C"),
        "inferred helper should now be emitted separately"
    );
}

#[test]
fn csharp_multi_entry_typed_trusts_render_as_block() {
    // Manifest with structured `trusts: {hashes:[...], groups:[...]}`
    // produces a typed list with 4 entries — too many for a single
    // line, so the C# header should break it into a `// trusts:`
    // block parallel to `// permissions:`.
    let nef_bytes = build_nef(&[0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "MultiTrust",
                "abi": {
                    "methods": [
                        {
                            "name": "main",
                            "parameters": [],
                            "returntype": "Void",
                            "offset": 0
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": {
                    "hashes": ["0xabc", "0xdef"],
                    "groups": ["02foo", "02bar"]
                }
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("// trusts:"),
        "multi-entry typed trusts should render as a block: {csharp}"
    );
    assert!(
        csharp.contains("//   hash:0xabc"),
        "hash entries should be indented under the trusts block: {csharp}"
    );
    assert!(
        csharp.contains("//   hash:0xdef"),
        "hash entries should be indented under the trusts block: {csharp}"
    );
    assert!(
        csharp.contains("//   group:02foo"),
        "group entries should be indented under the trusts block: {csharp}"
    );
    assert!(
        csharp.contains("//   group:02bar"),
        "group entries should be indented under the trusts block: {csharp}"
    );
    assert!(
        !csharp.contains("// trusts = [hash:0xabc, hash:0xdef, group:02foo, group:02bar]"),
        "multi-entry trusts must not stretch onto a single line: {csharp}"
    );
}

#[test]
fn header_surfaces_nef_compiler_and_source_fields() {
    // The NEF header carries `compiler` (always set in practice) and
    // `source` (often a repo URL or commit hash, sometimes empty).
    // Both fields are visible via `info` but were dropped from the
    // decompiled headers, leaving readers to run a separate command
    // to learn what produced the bytecode. Surface them as comment
    // lines under the script hash. Empty fields are silently
    // skipped (the test harness's `build_nef` writes `compiler =
    // "test"` and an empty source, so we exercise the present /
    // absent paths together).
    let nef_bytes = sample_nef();
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("    // compiler: test"),
        "high-level should surface the NEF compiler field:\n{high_level}"
    );
    assert!(
        !high_level.contains("    // source:"),
        "empty source should not emit a placeholder line:\n{high_level}"
    );

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("        // compiler: test"),
        "C# header should surface the NEF compiler field at the C# indent:\n{csharp}"
    );
    assert!(
        !csharp.contains("// source:"),
        "empty source should not emit a placeholder line in C#:\n{csharp}"
    );
}

#[test]
fn csharp_header_renders_method_tokens_block() {
    // The high-level renderer surfaces `// method tokens declared in
    // NEF` so a reader can cross-reference each CALLT call against
    // its native contract / call flags. The C# header silently
    // dropped the table, leaving readers to scrape the NEF
    // separately. Render it as a comment block (parity with the
    // existing `// permissions:` / `// groups:` blocks) — Neo
    // SmartContract Framework has no source-level construct for
    // method tokens, so a comment is the correct surface.
    //
    // Hash chosen to match the StdLib native contract so the
    // renderer adds the friendly contract label.
    let stdlib_hash: [u8; 20] = [
        0xC0, 0xEF, 0x39, 0xCE, 0xE0, 0xE4, 0xE9, 0x25, 0xC6, 0xC2, 0xA0, 0x6A, 0x79, 0xE1, 0x44,
        0x0D, 0xD8, 0x6F, 0xCE, 0xAC,
    ];
    let nef_bytes = build_nef_with_single_token(&[0x40], stdlib_hash, "Serialize", 1, true, 0x0F);

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("// method tokens declared in NEF:"),
        "C# header should open the method tokens comment block:\n{csharp}"
    );
    assert!(
        csharp.contains(
            "//   Serialize (StdLib::Serialize) hash=C0EF39CEE0E4E925C6C2A06A79E1440DD86FCEAC params=1 returns=true flags=0x0F"
        ),
        "method token line should match high-level layout (with native contract label):\n{csharp}"
    );
    assert!(
        csharp.contains("(ReadStates|WriteStates|AllowCall|AllowNotify)"),
        "call flags should be described:\n{csharp}"
    );
}

#[test]
fn csharp_header_omits_method_tokens_block_when_none() {
    // Empty token table => no header line at all (don't leave a
    // `// method tokens declared in NEF:` orphan).
    let nef_bytes = sample_nef();
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, None, OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        !csharp.contains("method tokens declared in NEF"),
        "no token block expected when NEF has no method tokens:\n{csharp}"
    );
}

#[test]
fn csharp_single_entry_typed_trusts_stay_on_one_line() {
    // Single-entry typed lists are short — keep them on one line so
    // the header doesn't grow unnecessarily for the common case.
    let nef_bytes = build_nef(&[0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "SingleTrust",
                "abi": {
                    "methods": [
                        {
                            "name": "main",
                            "parameters": [],
                            "returntype": "Void",
                            "offset": 0
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": { "groups": ["02abcdef"] }
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let csharp = decompilation.csharp.as_deref().expect("csharp output");
    assert!(
        csharp.contains("// trusts = [group:02abcdef]"),
        "single-entry trusts should stay compact on one line: {csharp}"
    );
    assert!(
        !csharp.contains("// trusts:"),
        "single-entry trusts should not break into a block: {csharp}"
    );
}

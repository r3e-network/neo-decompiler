use super::super::*;

#[test]
fn legacy_statement_to_csharp_converts_known_forms() {
    assert_eq!(legacy_statement_to_csharp("   "), "");
    assert_eq!(legacy_statement_to_csharp("// note"), "// note");
    assert_eq!(legacy_statement_to_csharp("let x = 1;"), "var x = 1;");
    // Helper rewrites must apply inside `let` initialisers too —
    // earlier the `let` branch early-returned before
    // `legacy_expression_to_csharp` ran, so `let t0 = min(x, y);` came
    // out as `var t0 = min(x, y);` (uncompilable).
    assert_eq!(
        legacy_statement_to_csharp("let t0 = min(x, y);"),
        "var t0 = BigInteger.Min(x, y);"
    );
    assert_eq!(
        legacy_statement_to_csharp("let t0 = is_null(loc0);"),
        "var t0 = (loc0 is null);"
    );
    assert_eq!(
        legacy_statement_to_csharp("let t0 = a cat b;"),
        "var t0 = a + b;"
    );
    // Helper rewrites must also apply inside throw / abort / assert
    // operands. Same bug class as the `let` branch fix — these
    // branches were extracting their bodies but not running them
    // through the expression rewriter.
    // Payloads use the same explicit conversion as structured rendering.
    assert_eq!(
        legacy_statement_to_csharp("throw(min(a, b));"),
        "throw new Exception(Convert.ToString(BigInteger.Min(a, b)));"
    );
    assert_eq!(
        legacy_statement_to_csharp("abort(\"err\" cat code);"),
        "throw new InvalidOperationException(Convert.ToString(\"err\" + code));"
    );
    assert_eq!(
        legacy_statement_to_csharp("assert(is_null(loc0));"),
        "global::Neo.SmartContract.Framework.ExecutionEngine.Assert((bool)(object)((loc0 is null)));"
    );
    // Assert messages use explicit conversion regardless of source type.
    assert_eq!(
        legacy_statement_to_csharp("assert(min(a, b) > 0, \"e\" cat code);"),
        "__NeoDecompilerAssertMessage((bool)(object)(BigInteger.Min(a, b) > 0), (string)(object)(\"e\" + code));"
    );
    assert_eq!(
        legacy_statement_to_csharp("assert(x > 0, code);"),
        "__NeoDecompilerAssertMessage((bool)(object)(x > 0), (string)(object)(code));"
    );
    assert_eq!(legacy_statement_to_csharp("if t0 {"), "if (t0) {");
    assert_eq!(legacy_statement_to_csharp("while t1 {"), "while (t1) {");
    assert_eq!(legacy_statement_to_csharp("loop {"), "while (true) {");
    assert_eq!(
        legacy_statement_to_csharp("else if loc0 < 3 {"),
        "else if (loc0 < 3) {"
    );
    assert_eq!(
        legacy_statement_to_csharp("} else if loc0 == 1 {"),
        "} else if (loc0 == 1) {"
    );
    assert_eq!(
        legacy_statement_to_csharp("for (let i = 0; i < 3; i++) {"),
        "for (var i = 0; i < 3; i++) {"
    );
    assert_eq!(
        legacy_statement_to_csharp("leave label_0x0010;"),
        "goto label_0x0010;"
    );
    // CAT operator (high-level pseudocode) → C# `+`. The translation
    // only fires for ` cat ` tokens outside string literals.
    assert_eq!(
        legacy_statement_to_csharp("return \"b:\" cat addr;"),
        "return \"b:\" + addr;"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = a cat b cat c;"),
        "var x = a + b + c;"
    );
    assert_eq!(
        legacy_statement_to_csharp("var msg = \"says cat ok\";"),
        "var msg = \"says cat ok\";"
    );
    // `throw(value);` (high-level pseudocode for NEO's THROW opcode)
    // becomes `throw new Exception(value);` in C# — NEO accepts any
    // stack value, but C# requires an `Exception`.
    assert_eq!(
        legacy_statement_to_csharp("throw(\"oops\");"),
        "throw new Exception(Convert.ToString(\"oops\"));"
    );
    // Non-string-literal identifiers use the same explicit coercion.
    assert_eq!(
        legacy_statement_to_csharp("throw(error_msg);"),
        "throw new Exception(Convert.ToString(error_msg));"
    );
    // ABORT / ABORTMSG stay visibly distinct from catchable THROW.
    assert_eq!(
        legacy_statement_to_csharp("abort();"),
        "throw new InvalidOperationException();"
    );
    assert_eq!(
        legacy_statement_to_csharp("abort(\"fatal\");"),
        "throw new InvalidOperationException(Convert.ToString(\"fatal\"));"
    );
    // Identifier operand — same wrapping rule as `throw(error_msg)`
    // since we don't know its static type.
    assert_eq!(
        legacy_statement_to_csharp("abort(reason);"),
        "throw new InvalidOperationException(Convert.ToString(reason));"
    );
    // ASSERT uses the framework API. ASSERTMSG uses a local opcode helper so
    // message validation remains eager instead of the framework's lazy JMPIF
    // + ABORTMSG lowering.
    assert_eq!(
        legacy_statement_to_csharp("assert(x > 0);"),
        "global::Neo.SmartContract.Framework.ExecutionEngine.Assert((bool)(object)(x > 0));"
    );
    assert_eq!(
        legacy_statement_to_csharp("assert(x > 0, \"must be positive\");"),
        "__NeoDecompilerAssertMessage((bool)(object)(x > 0), (string)(object)(\"must be positive\"));"
    );
    assert_eq!(
        legacy_statement_to_csharp("assert(1, 2);"),
        "__NeoDecompilerAssertMessage(1 != 0, (string)(object)(2));"
    );
    // Don't be fooled by commas inside the condition expression.
    assert_eq!(
        legacy_statement_to_csharp("assert(foo(a, b));"),
        "global::Neo.SmartContract.Framework.ExecutionEngine.Assert((bool)(object)(foo(a, b)));"
    );
    assert_eq!(
        legacy_statement_to_csharp("assert(null);"),
        "global::Neo.SmartContract.Framework.ExecutionEngine.Assert(false);"
    );
    // NEO arithmetic helpers — the high-level lift emits `abs/min/max/pow`
    // as bare function calls, but C# has no `abs` etc. in scope. Rewrite
    // to `BigInteger.X(...)`. For `pow`, the second argument must be
    // `int` per `BigInteger.Pow`'s signature.
    assert_eq!(
        legacy_statement_to_csharp("var x = abs(loc0);"),
        "var x = BigInteger.Abs(loc0);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = min(a, b);"),
        "var x = BigInteger.Min(a, b);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = max(a, b);"),
        "var x = BigInteger.Max(a, b);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = pow(base, exp);"),
        "var x = BigInteger.Pow(base, (int)(exp));"
    );
    // Literal exponent skips the redundant `(int)` cast — same idea
    // as `wrap_int_cast_unless_literal`. `pow(2, 8)` lifts cleanly to
    // `BigInteger.Pow(2, 8)` rather than `BigInteger.Pow(2, (int)(8))`.
    assert_eq!(
        legacy_statement_to_csharp("var x = pow(2, 8);"),
        "var x = BigInteger.Pow(2, 8);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = left(buf, 4);"),
        "var x = Helper.Left(buf, 4);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = substr(buf, 0, 16);"),
        "var x = Helper.Substr(buf, 0, 16);"
    );
    // Identifier-boundary respect: `mypow(x)` is NOT `pow(x)`.
    assert_eq!(
        legacy_statement_to_csharp("var x = mypow(2);"),
        "var x = mypow(2);"
    );
    // String-literal preservation: `"min(a)"` inside a string stays
    // verbatim.
    assert_eq!(
        legacy_statement_to_csharp("var x = \"min(a, b)\";"),
        "var x = \"min(a, b)\";"
    );
    // Nested helpers compose: `max(abs(a), b)` → `BigInteger.Max(BigInteger.Abs(a), b)`.
    assert_eq!(
        legacy_statement_to_csharp("var x = max(abs(a), b);"),
        "var x = BigInteger.Max(BigInteger.Abs(a), b);"
    );
    // Extended NEO arithmetic / buffer helpers — `BigInteger.X` for
    // ones .NET provides directly, `Helper.X` (Neo SmartContract
    // Framework) for the rest. Args at int-typed positions get an
    // `(int)(...)` cast so the C# overload signature matches.
    assert_eq!(
        legacy_statement_to_csharp("var x = sign(loc0);"),
        "var x = Helper.Sign(loc0);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = sqrt(loc0);"),
        "var x = Helper.Sqrt(loc0);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = modmul(a, b, m);"),
        "var x = Helper.ModMul(a, b, m);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = modpow(b, e, m);"),
        "var x = BigInteger.ModPow(b, e, m);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = within(v, lo, hi);"),
        "var x = Helper.Within(v, lo, hi);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = left(buf, n);"),
        "var x = Helper.Left(buf, (int)(n));"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = right(buf, n);"),
        "var x = Helper.Right(buf, (int)(n));"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = substr(buf, start, len);"),
        "var x = Helper.Substr(buf, (int)(start), (int)(len));"
    );
    // `is_null(x)` is a unary check, not a function call — it lifts
    // to the idiomatic C# pattern `(x is null)` instead of trying to
    // resolve a (non-existent) `IsNull` helper on the framework.
    assert_eq!(
        legacy_statement_to_csharp("if is_null(loc0) {"),
        "if ((loc0 is null)) {"
    );
    assert_eq!(
        legacy_statement_to_csharp("var x = is_null(loc0);"),
        "var x = (loc0 is null);"
    );
    // Nested into another helper: `if (!is_null(x))` style usages.
    assert_eq!(
        legacy_statement_to_csharp("var y = !is_null(loc0);"),
        "var y = !(loc0 is null);"
    );
    // Identifier-boundary respect: `assert_is_null(x)` must NOT pick
    // up the `is_null` rewrite (it's a different identifier).
    assert_eq!(
        legacy_statement_to_csharp("var x = my_is_null(loc0);"),
        "var x = my_is_null(loc0);"
    );
    // Empty collection constructors lifted from NEWMAP / NEWARRAY0 /
    // NEWSTRUCT0 — the lift emits `Map()`, `[]`, `Struct()` which
    // don't compile as-is. Rewrite to explicit `new` forms with
    // best-effort type defaults (`object` for Map's generic args
    // since we don't have key/value type info; `object[0]` for the
    // bare-literal array case).
    assert_eq!(
        legacy_statement_to_csharp("var t0 = Map();"),
        "var t0 = new Map<object, object>();"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = [];"),
        "var t0 = new object[0];"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = Struct();"),
        "var t0 = new Struct();"
    );
    // Identifier-boundary respect — a user-named `MyMap()` factory
    // must NOT be rewritten to `new MyMap<...>()`.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = MyMap();"),
        "var t0 = MyMap();"
    );
    // String-literal preservation — `"Map()"` inside a quoted
    // string stays verbatim.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = \"Map()\";"),
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
        legacy_statement_to_csharp("var t0 = new_buffer(8);"),
        "var t0 = new byte[8];"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = new_buffer(loc0);"),
        "var t0 = new byte[(int)(loc0)];"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = new_array(3);"),
        "var t0 = new object[3];"
    );
    // Negative literals also pass through without cast. Negative
    // sizes don't make sense for `new T[]` but the cast wouldn't
    // help anyway — `new T[-3]` and `new T[(int)(-3)]` are both
    // accepted by the C# compiler and reject at runtime alike.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = new_array(-3);"),
        "var t0 = new object[-3];"
    );
    // Identifier-boundary respect — `my_new_buffer(8)` is NOT the
    // NEWBUFFER lift output.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = my_new_buffer(8);"),
        "var t0 = my_new_buffer(8);"
    );
    // CONVERT lifts (`convert_to_bool` / `convert_to_integer` /
    // `convert_to_bytestring` / `convert_to_buffer`) — rewrite to
    // explicit C# casts.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = convert_to_bool(loc0);"),
        "var t0 = (bool)(loc0);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = convert_to_integer(loc0);"),
        "var t0 = (BigInteger)(loc0);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = convert_to_bytestring(loc0);"),
        "var t0 = (ByteString)(loc0);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = convert_to_buffer(loc0);"),
        "var t0 = (byte[])(loc0);"
    );
    // ISTYPE lifts — rewrite to C# pattern matches.
    assert_eq!(
        legacy_statement_to_csharp("if is_type_bool(loc0) {"),
        "if ((loc0 is bool)) {"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = is_type_integer(loc0);"),
        "var t0 = (loc0 is BigInteger);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = is_type_bytestring(loc0);"),
        "var t0 = (loc0 is ByteString);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = is_type_buffer(loc0);"),
        "var t0 = (loc0 is byte[]);"
    );
    // The other CONVERT / ISTYPE variants (any, pointer, array,
    // struct, map, interopinterface) deliberately keep the lifted
    // form — silently rewriting them would require type info the
    // lift doesn't supply. Leave a clear hint that the user has to
    // pick the right cast.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = convert_to_array(loc0);"),
        "var t0 = convert_to_array(loc0);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = is_type_map(loc0);"),
        "var t0 = is_type_map(loc0);"
    );
    // Collection helpers — `clear_items(c)`, `remove_item(c, k)`,
    // `keys(m)`, `values(m)`, `reverse_items(arr)` are NEO-flavoured
    // identifiers that don't compile. Rewrite to standard
    // .NET / Neo Map accessors.
    assert_eq!(
        legacy_statement_to_csharp("clear_items(loc0);"),
        "loc0.Clear();"
    );
    assert_eq!(
        legacy_statement_to_csharp("remove_item(loc0, key);"),
        "loc0.Remove(key);"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = keys(loc0);"),
        "var t0 = loc0.Keys;"
    );
    assert_eq!(
        legacy_statement_to_csharp("var t0 = values(loc0);"),
        "var t0 = loc0.Values;"
    );
    assert_eq!(
        legacy_statement_to_csharp("reverse_items(loc0);"),
        "loc0.Reverse();"
    );
    // Identifier-boundary respect — `my_keys(loc0)` is NOT KEYS.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = my_keys(loc0);"),
        "var t0 = my_keys(loc0);"
    );
    // APPEND lift — `append(arr, item)` → `arr.Add(item)`.
    assert_eq!(
        legacy_statement_to_csharp("append(loc0, 42);"),
        "loc0.Add(42);"
    );
    // HASKEY lift — `has_key(c, k)` → `c.ContainsKey(k)`.
    assert_eq!(
        legacy_statement_to_csharp("var t0 = has_key(loc0, key);"),
        "var t0 = loc0.ContainsKey(key);"
    );
    assert_eq!(
        legacy_statement_to_csharp("if has_key(loc0, key) {"),
        "if (loc0.ContainsKey(key)) {"
    );
}

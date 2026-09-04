import assert from "node:assert/strict";
import test from "node:test";

import {
  findBracketClose,
  findCallClose,
  nextOutsideMatch,
  splitCallArguments,
} from "../src/csharp-expression-scanner.js";
import { rewriteCSharpMethodTokenCalls } from "../src/csharp-framework.js";

const tokens = ["outer", "inner"].map((method, index) => ({
  hash: new Uint8Array(20).fill(index + 1),
  method,
  callFlags: 15,
}));

test("argument scanner distinguishes comparisons and shifts from generic types", () => {
  for (const comparison of ["left < right", "left > right", "left<=right", "left>=right", "left<<2", "left>>2"]) {
    assert.deepEqual(splitCallArguments(`${comparison}, next()`), [comparison, "next()"]);
  }
  assert.deepEqual(splitCallArguments("Map<string, List<int>> value, Action<int, string> callback", { typeDeclarations: true }), [
    "Map<string, List<int>> value", "Action<int, string> callback",
  ]);
  assert.deepEqual(splitCallArguments("factory<int, string>(1, 2), value"), [
    "factory<int, string>(1, 2)", "value",
  ]);
  assert.deepEqual(splitCallArguments("x<y, a>b"), ["x<y", "a>b"]);
  assert.deepEqual(splitCallArguments("x<y, a> b"), ["x<y", "a> b"]);
  assert.deepEqual(splitCallArguments("make<Map<int, List<string>>, List<int>>(), value"), [
    "make<Map<int, List<string>>, List<int>>()", "value",
  ]);
});

test("expression scanners treat comments and verbatim quoted text as opaque", () => {
  const call = 'f(1 /* ) , [ */ , @"a""),b", 2)';
  assert.equal(findCallClose(call, 1), call.length - 1);
  const bracket = '[1 /* ] */ , "]"]';
  assert.equal(findBracketClose(bracket, 0), bracket.length - 1);
  assert.deepEqual(splitCallArguments('1 /* ), [ */ , @"a""),b", 2'), [
    '1 /* ), [ */', '@"a""),b"', "2",
  ]);
  const text = '"hit()"; /* hit() */ // hit()\n hit()';
  assert.equal(nextOutsideMatch(text, /hit\(/g)?.index, text.lastIndexOf("hit("));
});

test("CALLT lowering visits nested tokens while preserving argument text exactly", () => {
  const source = 'return __callt_token_0(__callt_token_1(), 1 /* ) , */);';
  const output = rewriteCSharpMethodTokenCalls(source, tokens);
  assert.equal((output.match(/Contract\.Call\(/g) ?? []).length, 2, output);
  assert.doesNotMatch(output, /__callt_token_/);
  assert.match(output, /"outer".*"inner"/);
  assert.ok(output.endsWith(' }), 1 /* ) , */ });'), output);
});

test("CALLT lowering leaves comments, literals, and unrelated identifiers intact", () => {
  const examples = [
    'return "__callt_token_0()";',
    'return @"quoted "" __callt_token_0()";',
    'return 1; // __callt_token_0()',
    '/* __callt_token_0() */ return 1;',
    'return @__callt_token_0();',
    'return prefix__callt_token_0();',
    'return π__callt_token_0();',
    'return instance.__callt_token_0();',
    'return global::__callt_token_0();',
    'return __callt_token_00();',
    'return __callt_token_2();',
  ];
  for (const source of examples) {
    assert.equal(rewriteCSharpMethodTokenCalls(source, tokens), source);
  }
});

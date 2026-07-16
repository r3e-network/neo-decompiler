import assert from "node:assert/strict";
import test from "node:test";

import {
  foldConstantExpression,
} from "../src/csharp-expression-constant-evaluator.js";
import {
  foldConstantExpression as foldFromOriginalModule,
  rewriteConstantExpressions,
} from "../src/csharp-expression-constants.js";

test("constant evaluator folds complete integer and boolean expressions", () => {
  assert.equal(foldConstantExpression("(1 + 2) * 3"), "9");
  assert.equal(foldFromOriginalModule("1 + 2"), "3");
  assert.equal(foldConstantExpression("true && !false"), "true");
  assert.equal(foldConstantExpression("5 < 6"), "true");
});

test("constant evaluator refuses unsupported or faulting expressions", () => {
  assert.equal(foldConstantExpression("value + 1"), null);
  assert.equal(foldConstantExpression("1 / 0"), null);
  assert.equal(foldConstantExpression("1 << 1025"), null);
  assert.equal(foldConstantExpression('"a" + "b"'), null);
});

test("constant scanner rewrites statement-boundary expressions only", () => {
  assert.equal(rewriteConstantExpressions("return 1 + 1;"), "return 2;");
  assert.equal(rewriteConstantExpressions("let loc0 = (1 + 2) * 3;"), "let loc0 = 9;");
  assert.equal(
    rewriteConstantExpressions("let loc0 = value + 1 + 2;"),
    "let loc0 = value + 1 + 2;",
  );
});

test("constant scanner leaves quoted strings and comments untouched", () => {
  assert.equal(rewriteConstantExpressions('return "1 + 2"; // 3 + 4'), 'return "1 + 2"; // 3 + 4');
  assert.equal(rewriteConstantExpressions("/* 1 + 2 */ return 3 + 4;"), "/* 1 + 2 */ return 7;");
});

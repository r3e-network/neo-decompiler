import test from "node:test";
import assert from "node:assert/strict";
import { braceDelta, findBlockEnd, findMatchingClose } from "../src/postprocess/helpers.js";
import { collapseIfTrue } from "../src/postprocess/cleanup.js";

for (const comment of ["// {", "// }", '// " {', "// ' }"]) {
  test(`block matching ignores line comment: ${comment}`, () => {
    const statements = ["if true {", `    ${comment}`, "    work();", "}", "after();"];
    assert.equal(braceDelta(comment), 0);
    assert.equal(findBlockEnd(statements, 0), 3);
    assert.equal(findMatchingClose(statements, 0), 3);
    collapseIfTrue(statements);
    assert.deepEqual(statements, [`    ${comment}`, "    work();", "after();"]);
  });
}

test("brace delta counts code before trailing comments", () => {
  assert.equal(braceDelta("if true { // }"), 1);
  assert.equal(braceDelta("} // {"), -1);
  assert.equal(braceDelta('log("//"); } // {'), -1);
  assert.equal(braceDelta("log('//'); { // }"), 1);
  assert.equal(braceDelta(String.raw`log("escaped \" // {"); } // {`), -1);
});

test("constant-if cleanup preserves nested blocks around comment braces", () => {
  const statements = [
    "if true {", "    if flag {", "        // }", "        work();",
    "    }", "    afterInner();", "}", "after();",
  ];
  assert.equal(findBlockEnd(statements, 0), 6);
  collapseIfTrue(statements);
  assert.deepEqual(statements, [
    "    if flag {", "        // }", "        work();", "    }",
    "    afterInner();", "after();",
  ]);
});

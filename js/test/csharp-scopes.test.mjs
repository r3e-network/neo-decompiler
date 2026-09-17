import test from "node:test";
import assert from "node:assert/strict";
import { buildCSharpScopePlans } from "../src/csharp-scopes.js";
import { sourceBraceDelta } from "../src/csharp-source.js";
import { renderCSharpContract } from "../src/csharp.js";

function scopePlan(body) {
  const lines = ["fn sample() {", ...body, "}"];
  let depth = 0;
  const depths = lines.map((line) => {
    const current = depth;
    depth += sourceBraceDelta(line);
    return current;
  });
  return buildCSharpScopePlans(lines, depths);
}

for (const statement of [
  '    Runtime.Log("{");',
  "    // {",
  "    Runtime.Log('{');",
  '    Runtime.Log("escaped \\" {");',
]) {
  test(`scope plans ignore literal/comment opening braces: ${statement}`, () => {
    const body = [
      "  for (let loc0 = 0; loc0 < 3; loc0 = loc0 + 1) {",
      statement,
      "  }",
      "  return loc0;",
    ];
    const plan = scopePlan(body);
    assert.ok(plan.declarationsByStart.get(0)?.some(({ name }) => name === "loc0"));
    assert.match(plan.plansByLine.get(1), /for \(loc0 = 0;/);
    const output = renderCSharpContract([
      "contract Sample {", "fn sample() {", ...body, "}", "}",
    ].join("\n"));
    assert.doesNotMatch(output, /for \((?:BigInteger|dynamic|var) loc0/);
    assert.match(output, /(?:BigInteger|dynamic) loc0 = default;/);
  });
}

for (const statement of ['    Runtime.Log("}");', "    // }"]) {
  test(`scope plans keep loop-local declarations with ignored closing braces: ${statement}`, () => {
    const plan = scopePlan([
      "  for (let loc0 = 0; loc0 < 3; loc0 = loc0 + 1) {",
      statement,
      "    Runtime.Log(loc0);",
      "  }",
    ]);
    assert.equal(plan.declarationsByStart.size, 0);
  });
}

test("scope plans preserve ordered close/open braces on the same line", () => {
  const plan = scopePlan([
    "  for (let loc0 = 0; loc0 < 3; loc0 = loc0 + 1) {",
    "    if (true) {",
    "    } else {",
    "      Runtime.Log(loc0);",
    "    }",
    "  }",
    "  return loc0;",
  ]);
  assert.ok(plan.declarationsByStart.get(0)?.some(({ name }) => name === "loc0"));
});

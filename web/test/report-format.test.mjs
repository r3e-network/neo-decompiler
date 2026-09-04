import assert from "node:assert/strict";
import test from "node:test";

import { stringifyReport } from "../report-format.js";

test("browser reports retain exact positive and negative wide VM integers", () => {
  const report = {
    instructions: [
      { operand_value: { type: "I64", value: 9223372036854775807n } },
      { operand_value: { type: "I64", value: -9223372036854775808n } },
    ],
    offset: 1,
  };
  const displayed = JSON.parse(stringifyReport(report));
  assert.equal(displayed.instructions[0].operand_value.value, "9223372036854775807");
  assert.equal(displayed.instructions[1].operand_value.value, "-9223372036854775808");
  assert.equal(displayed.offset, 1);
});

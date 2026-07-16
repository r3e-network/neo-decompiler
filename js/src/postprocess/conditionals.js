import {
  extractIfCondition,
  findMatchingClose,
  isElseOpen,
  isIfOpen,
  leadingWhitespace,
} from "./helpers.js";

/** Normalize adjacent `else { if (...) {` blocks into `else if` chains. */
export function rewriteElseIfChains(statements) {
  let i = 0;
  while (i + 1 < statements.length) {
    if (isElseOpen(statements[i]) && isIfOpen(statements[i + 1])) {
      const condition = extractIfCondition(statements[i + 1]);
      if (condition !== null) {
        // Capture the original `else` wrapper's closer before removing its
        // nested `if` opener. The wrapper may contain suffix statements after
        // that nested branch, so its closer is not necessarily adjacent to
        // the nested branch's closer.
        const wrapperClose = findMatchingClose(statements, i);
        const nestedClose = findMatchingClose(statements, i + 1);
        if (wrapperClose < 0 || nestedClose < 0) {
          i++;
          continue;
        }
        // `else { if (condition) { ... } suffix; }` cannot be flattened to
        // `else if`: the suffix belongs to the enclosing else path and must
        // still run when the nested condition is false. Keep the wrapper so
        // control flow and side-effect ordering remain explicit.
        if (statements
          .slice(nestedClose + 1, wrapperClose)
          .some((line) => line.trim() !== "")) {
          i++;
          continue;
        }
        const parentIndent = leadingWhitespace(statements[i]);
        const nestedIndent = leadingWhitespace(statements[i + 1]);
        const dedent = Math.max(0, nestedIndent.length - parentIndent.length);
        if (dedent > 0) {
          for (let cursor = i + 2; cursor < nestedClose; cursor++) {
            const line = statements[cursor];
            const indent = leadingWhitespace(line);
            if (indent.length >= dedent) {
              statements[cursor] = `${indent.slice(dedent)}${line.trimStart()}`;
            }
          }
        }
        statements[i] = `${leadingWhitespace(statements[i])}} else if ${condition} {`;
        statements.splice(i + 1, 1);
        const adjustedNestedClose = nestedClose - 1;
        if (statements[adjustedNestedClose]?.trim() === "}") {
          statements.splice(adjustedNestedClose, 1);
        }
        continue;
      }
    }
    i++;
  }
}

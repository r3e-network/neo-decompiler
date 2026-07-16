import {
  findBlockEnd,
  isBlank,
  isIfOpen,
  leadingWhitespace,
  nextCodeLine,
} from "./helpers.js";

const GOTO_LABEL_RE = /^goto\s+(label_0x[\da-f]+);$/i;
const LABEL_LINE_RE = /^(label_0x[\da-f]+):$/i;
const DO_WHILE_END_RE = /^}\s+while\s+(?:!\((.+)\)|\((.+)\))\s*;?$/;

export function rewriteGotoDoWhile(statements) {
  let index = 0;
  while (index < statements.length) {
    const labelMatch = GOTO_LABEL_RE.exec(statements[index].trim());
    if (!labelMatch) { index++; continue; }
    const label = labelMatch[1];
    // `nextCodeLine` is inclusive; start after the transfer so the valid
    // `goto -> do { ... } while` compiler shape can be recognized.
    const doIdx = nextCodeLine(statements, index + 1);
    if (doIdx < 0 || statements[doIdx].trim() !== "do {") { index++; continue; }
    const endIdx = findBlockEnd(statements, doIdx);
    if (endIdx < 0) { index++; continue; }
    const condMatch = DO_WHILE_END_RE.exec(statements[endIdx].trim());
    if (!condMatch) { index++; continue; }
    const condition = condMatch[1] ? `!(${condMatch[1]})` : condMatch[2];
    const labelLine = `${label}:`;
    const labelIdx = statements.findIndex(
      (line, lineIndex) => lineIndex > doIdx && lineIndex < endIdx && line.trim() === labelLine,
    );
    if (labelIdx < 0) { index++; continue; }
    const setupLines = [];
    for (let i = labelIdx + 1; i < endIdx; i++) {
      if (!isBlank(statements[i])) setupLines.push(i);
    }
    statements[index] = "";
    statements[doIdx] = `${leadingWhitespace(statements[doIdx])}while ${condition} {`;
    statements[labelIdx] = "";
    if (setupLines.length === 0) {
      statements[endIdx] = `${leadingWhitespace(statements[endIdx])}}`;
    } else {
      const copies = setupLines.map((lineIndex) => statements[lineIndex]);
      for (let j = 0; j < copies.length; j++) statements.splice(doIdx + j, 0, copies[j]);
      statements[endIdx + copies.length] =
        `${leadingWhitespace(statements[endIdx + copies.length])}}`;
    }
    index++;
  }
}

export function rewriteIfGotoToWhile(statements) {
  let index = 0;
  while (index < statements.length) {
    const labelMatch = LABEL_LINE_RE.exec(statements[index].trim());
    if (!labelMatch) { index++; continue; }
    const label = labelMatch[1];
    let ifIdx = -1;
    for (let i = index + 1; i < statements.length; i++) {
      if (isIfOpen(statements[i].trim())) { ifIdx = i; break; }
    }
    if (ifIdx < 0) { index++; continue; }
    const endIdx = findBlockEnd(statements, ifIdx);
    if (endIdx < 0 || statements[endIdx].trim() !== "}") { index++; continue; }
    const gotoTarget = `goto ${label};`;
    let gotoIdx = -1;
    for (let i = ifIdx + 1; i < endIdx; i++) {
      if (statements[i].trim() === gotoTarget) { gotoIdx = i; break; }
    }
    if (gotoIdx < 0) { index++; continue; }
    const setupLines = statements
      .slice(index + 1, ifIdx)
      .filter((line) => !isBlank(line));
    statements[index] = "";
    statements[ifIdx] =
      `${leadingWhitespace(statements[ifIdx])}${statements[ifIdx].trim().replace(/^if /, "while ")}`;
    statements[gotoIdx] = "";
    for (let j = 0; j < setupLines.length; j++) statements.splice(endIdx + j, 0, setupLines[j]);
    index++;
  }
}

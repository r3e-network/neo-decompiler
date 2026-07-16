import { findCallClose, splitCallArguments } from "./csharp-expression-scanner.js";

const THROWING_UNDERFLOW_PLACEHOLDER =
  '(dynamic)(((object)null) ?? throw new InvalidOperationException("VM argument underflow: missing stack argument"))';

export function collectUnderflowTargets(lines) {
  const targets = new Set();
  for (const line of lines) {
    const match = line.match(
      /^\s*\/\/ warning: missing call argument values for ([A-Za-z_][A-Za-z0-9_]*) \(substituted \?\?\?\)/u,
    );
    if (match) targets.add(match[1]);
  }
  return targets;
}

export function rewriteUnderflowCallArguments(line, targets) {
  if (targets.size === 0 || !line.includes("default(dynamic)")) return line;
  let output = line;
  for (const target of targets) {
    const marker = `${target}(`;
    const open = output.indexOf(marker);
    if (open < 0) continue;
    const close = findCallClose(output, open + target.length);
    if (close < 0) continue;
    const args = splitCallArguments(output.slice(open + marker.length, close));
    const missing = args.findIndex((argument) => argument.includes("default(dynamic)"));
    if (missing < 0) continue;
    args[missing] = args[missing].replace(
      "default(dynamic)",
      THROWING_UNDERFLOW_PLACEHOLDER,
    );
    output = `${output.slice(0, open + marker.length)}${args.join(", ")}${output.slice(close)}`;
  }
  return output;
}

export function rewriteUnderflowWarningComment(line) {
  return line.includes("// warning: missing call argument values")
    ? line.replace("(substituted ???)", "(throwing compatibility expression)")
    : line;
}

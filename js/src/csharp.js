import { sanitizeIdentifier } from "./manifest.js";
import { nullableParametersForMethod } from "./csharp/nullability.js";
import {
  csharpIdentifier,
  escapeCSharpString,
  inferDeclarationTypes,
  isSafeManifestMethod,
  manifestMethodForName,
  renderBodyLine,
  renderEventDeclaration,
  renderMetadataLine,
  renderManifestAttributes,
  renderSignature,
} from "./csharp-render.js";

export function renderCSharpContract(highLevel, manifest = null, options = {}) {
  if (typeof highLevel !== "string") {
    throw new TypeError("highLevel must be a string");
  }

  const output = [
    "using System;",
    "using System.Numerics;",
    "using Neo.SmartContract.Framework;",
    "using Neo.SmartContract.Framework.Attributes;",
    "using Neo.SmartContract.Framework.Services;",
    "",
  ];
  let classSeen = false;
  const sourceLines = highLevel.split(/\r?\n/);
  const sourceDepthByLine = [];
  let sourceDepth = 0;
  for (const sourceLine of sourceLines) {
    sourceDepthByLine.push(sourceDepth);
    sourceDepth += sourceBraceDelta(sourceLine);
  }
  const labelsByLine = labelsVisibleInMethod(sourceLines, sourceDepthByLine);
  const declarationTypes = options.typedDeclarations
    ? inferDeclarationTypes(sourceLines)
    : null;
  const nullableParametersByLine = new Map();
  for (const [lineIndex, line] of sourceLines.entries()) {
    if (/^\s*fn\s+/.test(line)) {
      nullableParametersByLine.set(
        lineIndex,
        nullableParametersForMethod(sourceLines, lineIndex),
      );
    }
  }
  let metadataBlock = false;
  for (const [lineIndex, line] of sourceLines.entries()) {
    if (metadataBlock) {
      const indentation = line.match(/^\s*/)?.[0] ?? "";
      const trimmed = line.trim();
      output.push(trimmed ? `${indentation}// ${trimmed}` : line);
      if (trimmed === "}") metadataBlock = false;
      continue;
    }
    const contract = line.match(/^contract\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{$/);
    if (contract) {
      for (const attribute of renderManifestAttributes(manifest)) output.push(attribute);
      output.push(`public class ${csharpIdentifier(contract[1])} : SmartContract {`);
      classSeen = true;
      continue;
    }
    if (/^\s*fn\s+.*;(?:\s*\/\/.*)?$/.test(line)) {
      output.push(`${line.match(/^\s*/)?.[0] ?? ""}// ${line.trim()}`);
      continue;
    }
    if (/^\s*event\s+/.test(line)) {
      const event = renderEventDeclaration(line);
      output.push(event ?? `${line.match(/^\s*/)?.[0] ?? ""}// ${line.trim()}`);
      continue;
    }
    const signature = renderSignature(
      line,
      nullableParametersByLine.get(lineIndex) ?? new Set(),
    );
    if (signature) {
      const name = line.match(/^\s*fn\s+([A-Za-z_][A-Za-z0-9_]*)/)?.[1];
      const method = name ? manifestMethodForName(name, manifest) : null;
      const indentation = line.match(/^\s*/)?.[0] ?? "";
      if (method && sanitizeIdentifier(method.name) !== method.name) {
        output.push(`${indentation}[DisplayName("${escapeCSharpString(method.name)}")]`);
      }
      if (name && isSafeManifestMethod(name, manifest)) {
        output.push(`${indentation}[Safe]`);
      }
      output.push(signature);
      continue;
    }
    const metadata = renderMetadataLine(line);
    const orphanElse = /^\s*}\s*else\s*\{\s*$/.test(line) && sourceDepthByLine[lineIndex] <= 2;
    const renderedBody = renderBodyLine(line, declarationTypes);
    output.push(metadata ?? (orphanElse
      ? `${line.match(/^\s*/)?.[0] ?? ""}// orphaned else branch`
      : rewriteUnresolvedGotos(renderedBody, labelsByLine.get(lineIndex))));
    if (/^\s*(?:features|groups|permissions)\s*\{\s*$/.test(line)) {
      metadataBlock = true;
    }
  }
  if (!classSeen) {
    output.push("public class NeoContract : SmartContract {");
    output.push("    // high-level contract body was unavailable");
  }
  return output.join("\n").replace(/\n{3,}/g, "\n\n").trimEnd() + "\n";
}

// Labels are method-scoped in C#. A partially recovered VM branch can still
// leave a transfer to a label that was not emitted (or belonged to another
// method slice). Preserve valid transfers and turn only the unresolvable ones
// into explicit comments so the generated contract remains parseable.
function rewriteUnresolvedGotos(line, labels = new Set()) {
  const hasGoto = /\bgoto\s+(label_0x[0-9A-Fa-f]+);/i.test(line);
  if (!hasGoto) return line;
  const visibleLabels = labels.labels ?? labels;
  const labelDepths = labels.labelDepths ?? new Map();
  const finallyLabels = labels.finallyLabels ?? new Set();
  const currentDepth = labels.depth ?? Number.POSITIVE_INFINITY;
  let unresolved = false;
  const rewritten = line.replace(/\bgoto\s+(label_0x[0-9A-Fa-f]+);/gi, (full, label) => {
    const normalized = label.toLowerCase();
    if (
      visibleLabels.has(normalized) &&
      !finallyLabels.has(normalized) &&
      (labelDepths.get(normalized) ?? currentDepth) <= currentDepth
    ) {
      return full;
    }
    unresolved = true;
    return `/* unresolved control transfer: goto ${label}; */`;
  });
  if (!unresolved) return rewritten;
  return /^\s*goto\s+/.test(line)
    ? `${line.match(/^\s*/)?.[0] ?? ""}// unresolved control transfer: ${line.trim()}`
    : rewritten;
}

function labelsVisibleInMethod(lines, depths) {
  const labelsByLine = new Map();
  for (let start = 0; start < lines.length; start += 1) {
    if (!/^\s*fn\s+.*\{\s*$/.test(lines[start])) continue;
    const methodDepth = depths[start];
    let end = start + 1;
    while (end < lines.length) {
      if (depths[end] === methodDepth + 1 && /^\s*}\s*$/.test(lines[end])) break;
      end += 1;
    }
    const labels = new Set();
    const labelDepths = new Map();
    const finallyLabels = new Set();
    for (let index = start + 1; index < end; index += 1) {
      const match = lines[index].trim().match(/^(label_0x[0-9A-Fa-f]+):/i);
      if (match) {
        const normalized = match[1].toLowerCase();
        labels.add(normalized);
        labelDepths.set(normalized, depths[index]);
      }
    }
    for (let index = start + 1; index < end; index += 1) {
      if (!/\bfinally\s*\{\s*$/.test(lines[index])) continue;
      const finallyDepth = depths[index];
      for (let cursor = index + 1; cursor < end; cursor += 1) {
        if (depths[cursor] === finallyDepth + 1 && /^\s*}\s*$/.test(lines[cursor])) break;
        const match = lines[cursor].trim().match(/^(label_0x[0-9A-Fa-f]+):/i);
        if (match) finallyLabels.add(match[1].toLowerCase());
      }
    }
    for (let index = start; index <= end; index += 1) {
      labelsByLine.set(index, { labels, labelDepths, finallyLabels, depth: depths[index] });
    }
    start = end;
  }
  return labelsByLine;
}

function sourceBraceDelta(line) {
  let delta = 0;
  let quote = null;
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];
    if (quote) {
      if (character === "\\") index += 1;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === "/" && line[index + 1] === "/") break;
    if (character === '"' || character === "'") quote = character;
    else if (character === "{") delta += 1;
    else if (character === "}") delta -= 1;
  }
  return delta;
}

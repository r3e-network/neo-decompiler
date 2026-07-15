import { sanitizeIdentifier } from "./manifest.js";
import { nullableParametersForMethod } from "./csharp/nullability.js";
import { buildCSharpScopePlans } from "./csharp-scopes.js";
import {
  inferStaticSlotTypes,
  renderStaticSlotDeclarations,
  renderStaticSlotLine,
} from "./csharp-slots.js";
import {
  csharpIdentifier,
  csharpType,
  coerceCSharpReturn,
  escapeCSharpString,
  inferDeclarationTypes,
  isSafeManifestMethod,
  manifestMethodForName,
  renderBodyLine,
  renderEventDeclaration,
  renderMetadataLine,
  renderManifestAttributes,
  renderPatternComments,
  renderSignature,
} from "./csharp-render.js";
import { rewriteCSharpMethodTokenCalls } from "./csharp-framework.js";

export function renderCSharpContract(
  highLevel,
  manifest = null,
  options = {},
  patternInfo = null,
) {
  if (typeof highLevel !== "string") {
    throw new TypeError("highLevel must be a string");
  }

  const output = [
    "using System;",
    "using System.Numerics;",
    "using Neo.SmartContract.Framework;",
    "using Neo.SmartContract.Framework.Attributes;",
    "using Neo.SmartContract.Framework.Services;",
    "using Neo.SmartContract.Framework.Native;",
    "using LedgerContract = Neo.SmartContract.Framework.Native.Ledger;",
    "using NeoToken = Neo.SmartContract.Framework.Native.NEO;",
    "using GasToken = Neo.SmartContract.Framework.Native.GAS;",
    "using OracleContract = Neo.SmartContract.Framework.Native.Oracle;",
    "using PolicyContract = Neo.SmartContract.Framework.Native.Policy;",
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
  const nonVoidMethodInfo = analyzeNonVoidMethods(
    sourceLines,
    sourceDepthByLine,
  );
  const fallthroughGuardsByLine = nonVoidMethodInfo.guards;
  const scopePlans = buildCSharpScopePlans(
    sourceLines,
    sourceDepthByLine,
    options.typedDeclarations !== false,
  );
  const declarationTypesByLine = options.typedDeclarations === false
    ? null
    : inferDeclarationTypesByLine(sourceLines, sourceDepthByLine);
  const staticSlotTypes = options.typedDeclarations === false
    ? null
    : inferStaticSlotTypes(sourceLines, declarationTypesByLine);
  const returnTypesByLine = inferReturnTypesByLine(sourceLines, sourceDepthByLine);
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
  let patternCommentsPending = true;
  const patternComments = renderPatternComments(patternInfo);
  for (const [lineIndex, line] of sourceLines.entries()) {
    if (
      classSeen &&
      patternCommentsPending &&
      patternComments.length > 0 &&
      !metadataBlock &&
      !isContractHeaderLine(line)
    ) {
      output.push(...patternComments);
      patternCommentsPending = false;
    }
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
      output.push(...renderStaticSlotDeclarations(sourceLines, staticSlotTypes));
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
    if (nonVoidMethodInfo.bodyLines.has(lineIndex) && line.trim() === "return;") {
      const indentation = line.match(/^\s*/)?.[0] ?? "";
      output.push(`${indentation}throw new InvalidOperationException("Unreachable Neo VM fallthrough.");`);
      continue;
    }
    if (fallthroughGuardsByLine.has(lineIndex)) {
      const indentation = line.match(/^\s*/)?.[0] ?? "";
      output.push(`${indentation}    // unreachable VM fallthrough`);
      output.push(`${indentation}    throw new InvalidOperationException("Unreachable Neo VM fallthrough.");`);
    }
    const name = line.match(/^\s*fn\s+([A-Za-z_][A-Za-z0-9_]*)/)?.[1];
    const method = name ? manifestMethodForName(name, manifest) : null;
    const visibility = name && isInferredHelperName(name) && !method
      ? "private"
      : "public";
    const inferredHelper = name && isInferredHelperName(name) && !method;
    const unknownReturnType = inferredHelper
      ? "dynamic"
      : "object";
    const unknownParameterType = inferredHelper ? "dynamic" : "object";
    const signature = renderSignature(
      line,
      nullableParametersByLine.get(lineIndex) ?? new Set(),
      visibility,
      unknownReturnType,
      unknownParameterType,
    );
    if (signature) {
      const indentation = line.match(/^\s*/)?.[0] ?? "";
      if (method && sanitizeIdentifier(method.name) !== method.name) {
        output.push(`${indentation}[DisplayName("${escapeCSharpString(method.name)}")]`);
      }
      if (name && isSafeManifestMethod(name, manifest)) {
        output.push(`${indentation}[Safe]`);
      }
      output.push(signature);
      for (const declaration of scopePlans.declarationsByStart.get(lineIndex) ?? []) {
        output.push(`${indentation}    ${declaration.type} ${csharpIdentifier(declaration.name)} = default;`);
      }
      continue;
    }
    const metadata = renderMetadataLine(line);
    const orphanElse = /^\s*}\s*else\s*\{\s*$/.test(line) && sourceDepthByLine[lineIndex] <= 2;
    const scopedLine = rewriteCSharpMethodTokenCalls(
      scopePlans.plansByLine.get(lineIndex) ?? line,
      options.methodTokens,
    );
    const renderedBody = renderBodyLine(
      renderCSharpCatchClause(renderStaticSlotLine(scopedLine)),
      declarationTypesByLine?.get(lineIndex) ?? null,
    );
    const returnedBody = coerceCSharpReturn(
      renderedBody,
      returnTypesByLine.get(lineIndex) ?? null,
      declarationTypesByLine?.get(lineIndex) ?? null,
    );
    output.push(metadata ?? (orphanElse
      ? `${line.match(/^\s*/)?.[0] ?? ""}// orphaned else branch`
      : rewriteUnresolvedGotos(returnedBody, labelsByLine.get(lineIndex))));
    if (/^\s*(?:features|groups|permissions)\s*\{\s*$/.test(line)) {
      metadataBlock = true;
    }
  }
  if (classSeen && patternCommentsPending && patternComments.length > 0) {
    output.push(...patternComments);
  }
  if (!classSeen) {
    output.push("public class NeoContract : SmartContract {");
    output.push(...patternComments);
    output.push("    // high-level contract body was unavailable");
  }
  return output.join("\n").replace(/\n{3,}/g, "\n\n").trimEnd() + "\n";
}

// High-level lifting may recover a non-void method's body without a source
// `return` at the method boundary. This is common when a VM path terminates
// in ABORT/THROW or when a try/branch target could not be structured. Keep the
// generated C# valid while making the uncertainty explicit and fail-closed.
function analyzeNonVoidMethods(lines, depths) {
  const guards = new Set();
  const bodyLines = new Set();
  for (let start = 0; start < lines.length; start += 1) {
    const header = lines[start].match(
      /^\s*fn\s+[A-Za-z_][A-Za-z0-9_]*\(.*\)(?:\s*->\s*([^\s{]+))?\s*\{\s*$/,
    );
    if (!header || String(header[1] ?? "void").toLowerCase() === "void") continue;

    const methodDepth = depths[start];
    const end = findMethodEnd(lines, depths, start, methodDepth);
    if (end < 0) continue;

    for (let index = start + 1; index < end; index += 1) bodyLines.add(index);

    let lastTopLevelStatement = null;
    for (let index = start + 1; index < end; index += 1) {
      if (depths[index] !== methodDepth + 1) continue;
      const trimmed = lines[index].trim();
      if (!trimmed || trimmed.startsWith("//")) continue;
      lastTopLevelStatement = trimmed;
    }
    if (!isTerminalHighLevelStatement(lastTopLevelStatement)) guards.add(end);
    start = end;
  }
  return { guards, bodyLines };
}

function findMethodEnd(lines, depths, start, methodDepth = depths[start]) {
  for (let index = start + 1; index < lines.length; index += 1) {
    if (depths[index] === methodDepth + 1 && /^\s*}\s*$/.test(lines[index])) {
      return index;
    }
  }
  return -1;
}

function isTerminalHighLevelStatement(statement) {
  if (!statement) return false;
  return /^(?:return\b|throw\s*\(|abort(?:\s*\(|\s*;))/.test(statement);
}

function renderCSharpCatchClause(line) {
  return line.replace(/^(\s*\}\s*)catch\s*\{\s*$/u, "$1catch (Exception exception) {");
}

function isInferredHelperName(name) {
  return /^(?:sub|call)_0x[0-9A-Fa-f]+$/.test(name);
}

function inferDeclarationTypesByLine(lines, depths) {
  const typesByLine = new Map();
  for (let start = 0; start < lines.length; start += 1) {
    if (!/^\s*fn\s+.*\{\s*$/.test(lines[start])) continue;
    const methodDepth = depths[start];
    let end = start + 1;
    while (end < lines.length) {
      if (depths[end] === methodDepth + 1 && /^\s*}\s*$/.test(lines[end])) break;
      end += 1;
    }
    const methodTypes = inferDeclarationTypes(lines.slice(start, end + 1));
    for (let index = start + 1; index < end; index += 1) {
      typesByLine.set(index, methodTypes);
    }
    start = end;
  }
  return typesByLine;
}

function inferReturnTypesByLine(lines, depths) {
  const returnTypes = new Map();
  for (let start = 0; start < lines.length; start += 1) {
    if (!/^\s*fn\s+.*\{\s*$/.test(lines[start])) continue;
    const methodDepth = depths[start];
    const end = findMethodEnd(lines, depths, start, methodDepth);
    if (end < 0) continue;
    const raw = lines[start].match(/->\s*([^\s{]+)/)?.[1] ?? "void";
    const expected = raw.toLowerCase() === "any" ? "object" : csharpType(raw);
    for (let line = start + 1; line < end; line += 1) returnTypes.set(line, expected);
    start = end;
  }
  return returnTypes;
}

function isContractHeaderLine(line) {
  const trimmed = line.trim();
  return (
    trimmed === "" ||
    trimmed.startsWith("//") ||
    /^(?:supported_standards|features|groups|permissions|trusts)\b/.test(trimmed) ||
    trimmed.startsWith("pubkey=") ||
    /^fn\s+.*;(?:\s*\/\/.*)?$/.test(trimmed) ||
    /^event\s+/.test(trimmed)
  );
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

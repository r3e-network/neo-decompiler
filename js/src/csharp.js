import { sanitizeIdentifier } from "./manifest.js";
import { nullableParametersForMethod } from "./csharp/nullability.js";
import { buildCSharpScopePlans } from "./csharp-scopes.js";
import {
  analyzeNonVoidMethods,
  inferDeclarationTypesByLine,
  inferReturnTypesByLine,
} from "./csharp-method-analysis.js";
import { labelsVisibleInMethod, rewriteUnresolvedGotos } from "./csharp-labels.js";
import { isContractHeaderLine, sourceBraceDelta } from "./csharp-source.js";
import {
  inferStaticSlotTypes,
  renderStaticSlotDeclarations,
  renderStaticSlotLine,
} from "./csharp-slots.js";
import {
  csharpIdentifier,
  coerceCSharpReturn,
  escapeCSharpString,
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
import { isInsideQuotedString } from "./csharp-expression-scanner.js";

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
  const catchScopes = buildCatchScopes(sourceLines, sourceDepthByLine);
  const catchVariablesByHeader = new Map(
    catchScopes.map((scope) => [scope.headerLine, scope.variable]),
  );
  const catchVariablesByBodyLine = new Map();
  for (const scope of catchScopes) {
    for (let lineIndex = scope.startLine; lineIndex < scope.endLine; lineIndex += 1) {
      const previous = catchVariablesByBodyLine.get(lineIndex);
      if (!previous || previous.startLine < scope.startLine) {
        catchVariablesByBodyLine.set(lineIndex, scope);
      }
    }
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
    const labelContext = labelsByLine.get(lineIndex);
    if (labelContext?.skipLabel && /^\s*label_0x[0-9A-Fa-f]+:\s*$/.test(line)) {
      continue;
    }
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
    const catchScope = catchVariablesByBodyLine.get(lineIndex);
    const bodyLine = catchScope
      ? replaceExceptionReference(scopedLine, catchScope.variable)
      : scopedLine;
    const renderedBody = renderBodyLine(
      renderCSharpCatchClause(
        renderStaticSlotLine(bodyLine),
        catchVariablesByHeader.get(lineIndex) ?? "exception",
      ),
      declarationTypesByLine?.get(lineIndex) ?? null,
    );
    const returnedBody = coerceCSharpReturn(
      renderedBody,
      returnTypesByLine.get(lineIndex) ?? null,
      declarationTypesByLine?.get(lineIndex) ?? null,
    );
    output.push(metadata ?? (orphanElse
      ? `${line.match(/^\s*/)?.[0] ?? ""}// orphaned else branch`
      : rewriteUnresolvedGotos(returnedBody, labelContext)));
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

function renderCSharpCatchClause(line, exceptionName = "exception") {
  return line.replace(
    /^(\s*\}\s*)catch\s*\{\s*$/u,
    `$1catch (Exception ${exceptionName}) {`,
  );
}

function buildCatchScopes(lines, depths) {
  const scopes = [];
  let ordinal = 0;
  for (let index = 0; index < lines.length; index += 1) {
    if (/^\s*fn\s+/.test(lines[index])) ordinal = 0;
    if (!/^\s*\}\s*catch\s*\{\s*$/u.test(lines[index]) &&
        !/^\s*catch\s*\{\s*$/u.test(lines[index])) {
      continue;
    }
    const variable = ordinal === 0 ? "exception" : `exception_${ordinal}`;
    ordinal += 1;
    const bodyDepth = depths[index] + sourceBraceDelta(lines[index]);
    let endLine = lines.length;
    for (let cursor = index + 1; cursor < lines.length; cursor += 1) {
      const trimmed = lines[cursor].trim();
      if (depths[cursor] < bodyDepth ||
          (depths[cursor] === bodyDepth && /^\}\s*(?:catch|finally|else)\s*\{/u.test(trimmed))) {
        endLine = cursor;
        break;
      }
    }
    scopes.push({ headerLine: index, startLine: index + 1, endLine, variable });
  }
  return scopes;
}

function replaceExceptionReference(line, variable) {
  if (variable === "exception" || line.trim().startsWith("//")) return line;
  const pattern = /\bexception\b/gu;
  let cursor = 0;
  let output = "";
  let match;
  while ((match = pattern.exec(line)) !== null) {
    if (isInsideQuotedString(line, match.index)) continue;
    output += line.slice(cursor, match.index) + variable;
    cursor = match.index + match[0].length;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function isInferredHelperName(name) {
  return /^(?:sub|call)_0x[0-9A-Fa-f]+$/.test(name);
}

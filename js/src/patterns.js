import { SYSCALLS } from "./generated/syscalls.js";

/**
 * Conservative contract-standard and source-language pattern identification.
 * Manifest declarations are authoritative; ABI and bytecode signals remain
 * explicit evidence instead of being presented as certain facts.
 */
export function identifyPatterns(nef, instructions, manifest = null) {
  const standards = new Set();
  const patterns = new Set();
  const evidence = [];
  let declaredStandard = false;

  for (const standard of manifest?.supportedStandards ?? []) {
    const normalized = String(standard).trim().toUpperCase();
    if (!normalized) continue;
    standards.add(normalized);
    declaredStandard = true;
    evidence.push({ source: "manifest.supportedstandards", value: normalized });
    if (normalized.startsWith("NEP-")) patterns.add(normalized);
  }

  const methodNames = new Set(
    (manifest?.abi?.methods ?? []).map((method) => String(method.name).toLowerCase()),
  );
  const has = (...names) => names.every((name) => methodNames.has(name.toLowerCase()));
  if (has("symbol", "decimals", "totalSupply", "balanceOf", "transfer")) {
    standards.add("NEP-17");
    patterns.add("NEP-17");
    evidence.push({
      source: "manifest.abi.methods",
      value: "symbol,decimals,totalSupply,balanceOf,transfer",
    });
  }
  if (has("ownerOf", "tokensOf", "transfer")) {
    standards.add("NEP-11");
    patterns.add("NEP-11");
    evidence.push({ source: "manifest.abi.methods", value: "ownerOf,tokensOf,transfer" });
  }
  if ((manifest?.abi?.events ?? []).some((event) => event.name?.toLowerCase() === "transfer")) {
    evidence.push({ source: "manifest.abi.events", value: "Transfer" });
  }
  const permissions = manifest?.permissions ?? [];
  if (permissions.length > 0) {
    patterns.add("call_permissions");
    evidence.push({ source: "manifest.permissions", value: String(permissions.length) });
  }
  if (permissions.some((permission) =>
    permission.contract === "*" || permission.methods === "*"
  )) {
    patterns.add("wildcard_permissions");
    evidence.push({ source: "manifest.permissions", value: "wildcard" });
  }

  const syscallNames = new Set();
  for (const instruction of instructions ?? []) {
    if (instruction.opcode?.mnemonic !== "SYSCALL") continue;
    const name = SYSCALLS.get(instruction.operand?.value)?.name;
    if (name) syscallNames.add(name);
  }
  for (const name of syscallNames) {
    if (name.startsWith("System.Storage.")) {
      patterns.add("storage");
      evidence.push({ source: "syscall", value: name });
    }
    if (name === "System.Runtime.Notify" || name === "System.Runtime.Log") {
      patterns.add("notifications");
      evidence.push({ source: "syscall", value: name });
    }
  }

  const compiler = nef?.header?.compiler?.trim() || null;
  if (compiler) evidence.push({ source: "nef.header.compiler", value: compiler });
  if (nef?.header?.source?.trim()) {
    evidence.push({ source: "nef.header.source", value: nef.header.source });
  }
  const language = inferLanguage(compiler) ?? inferLanguageFromSource(nef?.header?.source);
  const confidence = declaredStandard
    ? "high"
    : patterns.size > 0 || language
      ? "medium"
      : "unknown";
  return {
    standards: [...standards].sort(),
    patterns: [...patterns].sort(),
    language,
    compiler,
    confidence,
    evidence,
  };
}

function inferLanguage(compiler) {
  if (!compiler) return null;
  const value = compiler.toLowerCase();
  if (value.includes("csharp") || value.includes("neo.compiler")) return "C#";
  if (value.includes("boa") || value.includes("python")) return "Python";
  if (value.includes("neogo") || value.includes("neo-go")) return "Go";
  if (value.includes("typescript") || value.includes("javascript")) return "TypeScript/JavaScript";
  return null;
}

function inferLanguageFromSource(source) {
  const value = String(source ?? "").toLowerCase();
  if (value.endsWith(".cs")) return "C#";
  if (value.endsWith(".py")) return "Python";
  if (value.endsWith(".go")) return "Go";
  if (value.endsWith(".ts") || value.endsWith(".js")) return "TypeScript/JavaScript";
  return null;
}

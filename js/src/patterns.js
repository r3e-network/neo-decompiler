import { SYSCALLS } from "./generated/syscalls.js";
import { describeMethodToken } from "./native-contracts.js";
import { inferSyscallPatterns } from "./patterns-syscalls.js";

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

  const declaredStandards = Array.isArray(manifest?.supportedStandards)
    ? manifest.supportedStandards
    : Array.isArray(manifest?.supportedstandards)
      ? manifest.supportedstandards
      : [];
  for (const standard of declaredStandards) {
    const normalized = String(standard).trim().toUpperCase();
    if (!normalized) continue;
    standards.add(normalized);
    declaredStandard = true;
    evidence.push({ source: "manifest.supportedstandards", value: normalized });
    if (normalized.startsWith("NEP-")) patterns.add(normalized);
  }

  const methods = Array.isArray(manifest?.abi?.methods) ? manifest.abi.methods : [];
  const methodNames = new Set(
    methods
      .filter((method) => method && typeof method === "object")
      .map((method) => String(method.name ?? "").toLowerCase()),
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
  const hasOwnerAccessor = methodNames.has("owner") || methodNames.has("getowner");
  const hasOwnershipOperation = ["verify", "setowner", "transferownership"].some((name) =>
    methodNames.has(name),
  );
  if (hasOwnerAccessor && hasOwnershipOperation) {
    patterns.add("ownership");
    evidence.push({
      source: "manifest.abi.methods",
      value: "owner,verify/transferOwnership",
    });
  }
  if (methodNames.has("mint")) {
    patterns.add("minting");
    evidence.push({ source: "manifest.abi.methods", value: "mint" });
  }
  if (methodNames.has("burn")) {
    patterns.add("burning");
    evidence.push({ source: "manifest.abi.methods", value: "burn" });
  }
  if (methodNames.has("pause") && methodNames.has("unpause")) {
    patterns.add("pausable");
    evidence.push({ source: "manifest.abi.methods", value: "pause,unpause" });
  }
  if (methodNames.has("royaltyinfo")) {
    standards.add("NEP-24");
    patterns.add("NEP-24");
    patterns.add("royalties");
    evidence.push({ source: "manifest.abi.methods", value: "royaltyInfo" });
  }
  for (const [name, label] of [
    ["onnep17payment", "onNEP17Payment"],
    ["onnep11payment", "onNEP11Payment"],
  ]) {
    if (methodNames.has(name)) {
      patterns.add("token_receiver");
      evidence.push({ source: "manifest.abi.methods", value: label });
    }
  }
  const events = Array.isArray(manifest?.abi?.events) ? manifest.abi.events : [];
  if (events.length > 0) {
    patterns.add("events");
    evidence.push({ source: "manifest.abi.events", value: String(events.length) });
  }
  const hasTransferEvent = events.some((event) => event?.name?.toLowerCase() === "transfer");
  if (hasTransferEvent) {
    evidence.push({ source: "manifest.abi.events", value: "Transfer" });
    if (methodNames.has("transfer")) {
      patterns.add("token_transfers");
      evidence.push({ source: "manifest.abi.methods", value: "transfer + Transfer" });
    }
  }
  const permissions = Array.isArray(manifest?.permissions) ? manifest.permissions : [];
  if (permissions.length > 0) {
    patterns.add("call_permissions");
    evidence.push({ source: "manifest.permissions", value: String(permissions.length) });
  }
  if (permissions.some((permission) =>
    permission && (permission.contract === "*" || permission.methods === "*")
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
  if ((nef?.methodTokens ?? []).length > 0) {
    patterns.add("method_tokens");
    evidence.push({ source: "nef.method_tokens", value: String(nef.methodTokens.length) });
  }
  for (const token of nef?.methodTokens ?? []) {
    const hash = token.hash instanceof Uint8Array
      ? token.hash
      : Array.isArray(token.hash) && token.hash.length === 20
        ? Uint8Array.from(token.hash)
        : null;
    if (!hash) continue;
    const hint = describeMethodToken(hash, token.method);
    if (hint?.hasExactMethod()) {
      patterns.add("native_contract_calls");
      evidence.push({
        source: "nef.method_tokens.native",
        value: hint.formattedLabel(token.method),
      });
      if (hint.contract === "OracleContract") patterns.add("oracle");
      if (hint.contract === "ContractManagement") {
        patterns.add("contract_management");
        if (hint.canonicalMethod === "Update") patterns.add("upgradeable");
      }
      if (hint.contract === "Governance") patterns.add("governance");
      if (hint.contract === "RoleManagement") patterns.add("role_management");
      if (hint.contract === "PolicyContract") patterns.add("policy_management");
      if (hint.contract === "TokenManagement") patterns.add("token_management");
      if (hint.contract === "LedgerContract") patterns.add("ledger");
      if (hint.contract === "Notary") patterns.add("notary");
      if (hint.contract === "Treasury") patterns.add("treasury");
    }
  }
  if ((instructions ?? []).some((instruction) =>
    instruction.opcode?.mnemonic === "CALLA" ||
    instruction.opcode?.mnemonic === "CALLT" ||
    SYSCALLS.get(instruction.operand?.value)?.name === "System.Contract.Call"
  )) {
    patterns.add("external_calls");
    evidence.push({ source: "bytecode.calls", value: "CALLA/CALLT/Contract.Call" });
  }
  for (const name of syscallNames) {
    inferSyscallPatterns(name, patterns, evidence);
  }

  const compiler = nef?.header?.compiler?.trim() || null;
  if (compiler) evidence.push({ source: "nef.header.compiler", value: compiler });
  if (nef?.header?.source?.trim()) {
    evidence.push({ source: "nef.header.source", value: nef.header.source });
  }
  const compilerLanguage = inferLanguage(compiler);
  const language = compilerLanguage ?? inferLanguageFromSource(nef?.header?.source);
  const strongInferredPattern = patterns.has("NEP-17") || patterns.has("NEP-11");
  const confidence = declaredStandard
    ? "high"
    : compilerLanguage || strongInferredPattern || (evidence.length >= 2 && patterns.size > 0)
      ? "medium"
      : evidence.length > 0
        ? "low"
        : "unknown";
  evidence.sort((left, right) =>
    compareCodepoints(left.source, right.source) || compareCodepoints(left.value, right.value),
  );
  return {
    standards: [...standards].sort(),
    patterns: [...patterns].sort(),
    language,
    compiler,
    confidence,
    evidence,
  };
}

function compareCodepoints(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

function inferLanguage(compiler) {
  if (!compiler) return null;
  const value = compiler.toLowerCase();
  if (value.includes("csharp") || value.includes("neo.compiler")) return "C#";
  if (value.includes("boa") || value.includes("python")) return "Python";
  if (value.includes("neogo") || value.includes("neo-go")) return "Go";
  if (value.includes("rust")) return "Rust";
  if (value.includes("typescript") || value.includes("javascript")) return "TypeScript/JavaScript";
  if (value.includes("java")) return "Java";
  return null;
}

function inferLanguageFromSource(source) {
  const value = String(source ?? "").toLowerCase();
  const withoutSuffix = value.split(/[?#]/, 1)[0];
  const filename = withoutSuffix.split(/[\\/]/).at(-1) ?? withoutSuffix;
  if (filename.endsWith(".cs") || filename.endsWith(".csproj")) return "C#";
  if (filename.endsWith(".py")) return "Python";
  if (filename.endsWith(".go")) return "Go";
  if (filename.endsWith(".rs")) return "Rust";
  if (filename.endsWith(".java")) return "Java";
  if (
    filename.endsWith(".ts") ||
    filename.endsWith(".tsx") ||
    filename.endsWith(".js") ||
    filename.endsWith(".jsx")
  ) return "TypeScript/JavaScript";
  return null;
}

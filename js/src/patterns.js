import { SYSCALLS } from "./generated/syscalls.js";
import { describeMethodToken } from "./native-contracts.js";
import { inferNativePatterns } from "./patterns-native.js";
import { inferSyscallPatterns } from "./patterns-syscalls.js";

/**
 * Conservative contract-standard and C# target metadata identification.
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
  const hasTransferEvent = events.some((event) =>
    typeof event?.name === "string" && event.name.toLowerCase() === "transfer",
  );
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
  if (permissions.some((permission) => {
    if (!permission || typeof permission !== "object") return false;
    // Rust's tolerant manifest model treats every string selector as the
    // wildcard enum (strict parsing validates that the value is "*"). An
    // omitted methods field also defaults to that wildcard variant.
    const wildcardMethods =
      permission.methods === undefined || typeof permission.methods === "string";
    return permission.contract === "*" || wildcardMethods;
  })) {
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
      const label = hint.formattedLabel(token.method);
      evidence.push({
        source: "nef.method_tokens.native",
        value: label,
      });
      inferNativePatterns(hint, label, patterns, evidence);
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

  // Backward relative jumps are a structural signal for loop / iteration shapes.
  const jumpOpcodes = new Set([
    "JMP",
    "JMP_L",
    "JMPIF",
    "JMPIF_L",
    "JMPIFNOT",
    "JMPIFNOT_L",
    "JMPEQ",
    "JMPEQ_L",
    "JMPNE",
    "JMPNE_L",
    "JMPLT",
    "JMPLT_L",
    "JMPLE",
    "JMPLE_L",
    "JMPGT",
    "JMPGT_L",
    "JMPGE",
    "JMPGE_L",
  ]);
  if (
    Array.isArray(instructions) &&
    instructions.some((instruction) => {
      const opcodeName =
        typeof instruction?.opcode === "string"
          ? instruction.opcode
          : instruction?.opcode?.mnemonic ?? "";
      if (!jumpOpcodes.has(String(opcodeName).toUpperCase())) {
        return false;
      }
      const operand = instruction.operand;
      if (typeof operand === "number") return operand < 0;
      if (
        operand &&
        typeof operand === "object" &&
        (operand.kind === "Jump" || operand.kind === "Jump32") &&
        typeof operand.value === "number"
      ) {
        return operand.value < 0;
      }
      return false;
    })
  ) {
    patterns.add("loops");
    evidence.push({ source: "bytecode.control_flow", value: "backward jump" });
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

// C# is the only generated source target. Keep language metadata for report
// parity, but do not claim support for other compiler families without a
// corresponding renderer.
function inferLanguage(compiler) {
  if (!compiler) return null;
  const value = String(compiler).trim().toLowerCase();
  if (!value) return null;
  // Fixed-width NEF compiler tags may be short (`cs`, `cs__`) rather than full names.
  // Match complete tokens so values such as `notcsharp` cannot claim C# support.
  if (value.split(/[^a-z0-9]+/).some((token) => token === "csharp" || token === "cs")) {
    return "C#";
  }
  return null;
}

function inferLanguageFromSource(source) {
  const value = String(source ?? "").toLowerCase();
  const withoutSuffix = value.split(/[?#]/, 1)[0];
  const filename = withoutSuffix.split(/[\\/]/).at(-1) ?? withoutSuffix;
  if (filename.endsWith(".cs") || filename.endsWith(".csproj")) return "C#";
  return null;
}

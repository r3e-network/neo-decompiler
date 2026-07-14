// Type declarations for neo-decompiler-js

// ─── Error classes ─────────────────────────────────────────────────────────

export class NeoDecompilerError extends Error {
  name: string;
  details: Record<string, unknown>;
  constructor(message: string, details?: Record<string, unknown>);
}

export class NefParseError extends NeoDecompilerError {}

export class DisassemblyError extends NeoDecompilerError {}

export class ManifestParseError extends NeoDecompilerError {}

// ─── Core data types ───────────────────────────────────────────────────────

export interface OperandEncoding {
  kind: string;
  size?: number;
}

export interface OpCode {
  name: string;
  mnemonic: string;
  byte: number;
  operandEncoding: OperandEncoding | null;
}

export interface Operand {
  kind: string;
  value: number | bigint | string | Uint8Array;
}

export interface Instruction {
  offset: number;
  opcode: OpCode;
  operand: Operand | null;
}

export interface MethodToken {
  hash: Uint8Array;
  method: string;
  parametersCount: number;
  hasReturnValue: boolean;
  callFlags: number;
}

export interface NefHeader {
  magic: string;
  compiler: string;
  source: string;
}

export interface NefFile {
  script: Uint8Array;
  header: NefHeader;
  methodTokens: MethodToken[];
  /** Canonical display ("big-endian", 0x-prefixed explorer) form of the
   * Hash160 script hash, as uppercase hex without the `0x` prefix. */
  scriptHash: string;
  /** Little-endian (Neo internal `UInt160`) form of the script hash — the
   * raw `RIPEMD160(SHA256(script))` digest, as uppercase hex. */
  scriptHashLE: string;
  checksum: number;
}

// ─── Manifest types ────────────────────────────────────────────────────────

export interface ManifestParameter {
  name: string;
  kind: string;
}

export interface ManifestMethod {
  name: string;
  parameters: ManifestParameter[];
  returnType: string;
  offset: number | null;
  safe: boolean;
}

export interface ManifestEvent {
  name: string;
  parameters: ManifestParameter[];
}

export interface ManifestAbi {
  methods: ManifestMethod[];
  events: ManifestEvent[];
}

/**
 * Raw `features` object from the manifest. Neo N3 requires this to be
 * an empty object (the legacy 2.x `storage`/`payable` flags do not
 * exist in N3); tolerant parsing surfaces whatever a malformed
 * manifest declared, and strict parsing rejects non-empty content.
 */
export type ManifestFeatures = Record<string, unknown>;

export interface ManifestGroup {
  /** Hex-encoded compressed public key (33 bytes / 66 hex chars). */
  pubkey: string;
  /** Base64-encoded signature over the contract script hash. */
  signature: string;
}

/**
 * Permission entry from `manifest.permissions`. The official manifest
 * encoding uses a plain string for `contract`: the wildcard `"*"`, a
 * 0x-prefixed 20-byte contract hash (42 chars), or a 33-byte group
 * public key (66 hex chars). Any other value — including the
 * non-official `{hash}`/`{group}` object forms — is a malformed
 * descriptor (rejected in strict mode, kept verbatim in tolerant
 * mode). Use `classifyPermissionContract` to classify by shape.
 * Methods is either a string array or the wildcard `"*"`.
 */
export interface ManifestPermission {
  contract: unknown;
  methods: string[] | string;
}

/**
 * Shape classification of a permission `contract` descriptor,
 * mirroring the official `ContractPermissionDescriptor.FromJson()`.
 */
export type PermissionContractClassification =
  | { kind: "wildcard"; value: "*" }
  | { kind: "hash"; hash: string }
  | { kind: "group"; group: string }
  | { kind: "other"; value: unknown };

/**
 * Classify a permission `contract` descriptor by shape: `"*"` →
 * wildcard, 42-char `0x`-hex → hash, 66-char hex → group, anything
 * else → other (malformed).
 */
export function classifyPermissionContract(
  value: unknown,
): PermissionContractClassification;

/**
 * `manifest.trusts` may be:
 * - `null` (field absent),
 * - the string `"*"` (wildcard — any contract may be trusted),
 * - an array of hash strings (legacy bare-hash list), or
 * - an object `{ hashes?: string[]; groups?: string[] }` (structured form).
 */
export type ManifestTrusts =
  | null
  | "*"
  | string[]
  | { hashes?: string[]; groups?: string[] };

export interface ContractManifest {
  name: string;
  groups: ManifestGroup[];
  supportedStandards: string[];
  features: ManifestFeatures;
  abi: ManifestAbi;
  permissions: ManifestPermission[];
  trusts: ManifestTrusts;
  /**
   * Free-form metadata from `manifest.extra` (e.g. `Author`, `Email`).
   * The spec allows any JSON value, so this is left as `unknown`.
   * The high-level renderer stringifies string/number/boolean scalars
   * and silently drops nested objects/arrays/null.
   */
  extra: unknown;
}

// ─── Method groups ─────────────────────────────────────────────────────────

export interface MethodGroup {
  start: number;
  end: number;
  name: string;
  source: ManifestMethod | null;
  instructions: Instruction[];
}

// ─── Analysis types ────────────────────────────────────────────────────────

export interface MethodRef {
  offset: number;
  name: string;
}

/**
 * Direct CALL / CALL_L edge, or a CALLA edge whose function pointer was
 * resolved to a known offset. `method` falls back to a synthesized
 * `sub_0xNNNN` ref when the offset isn't a known method start.
 */
export interface InternalCallTarget {
  kind: "Internal";
  method: MethodRef;
}

/** SYSCALL edge. */
export interface SyscallCallTarget {
  kind: "Syscall";
  /** Little-endian u32 syscall hash as decoded from the operand. */
  hash: number;
  /** Resolved syscall name, or `null` when the hash is unknown. */
  name: string | null;
  /**
   * Whether the syscall pushes a return value. Unknown hashes default
   * to `true`.
   */
  returnsValue: boolean;
}

/** CALLT edge whose token index resolved inside `nef.methodTokens`. */
export interface MethodTokenCallTarget {
  kind: "MethodToken";
  /** Index into `nef.methodTokens` (the CALLT operand). */
  index: number;
  /** Token hash in little-endian byte order (uppercase hex). */
  hashLe: string;
  /** Token hash in big-endian byte order (uppercase hex). */
  hashBe: string;
  method: string;
  parametersCount: number;
  hasReturnValue: boolean;
  callFlags: number;
}

/**
 * Unresolvable call: a CALLT whose token index is out of range
 * (`operand` is the index), or a CALLA whose function pointer could
 * not be traced (`operand` is `null`).
 */
export interface IndirectCallTarget {
  kind: "Indirect";
  opcode: string;
  operand: number | null;
}

/**
 * Call-graph edge target, discriminated on `kind`. Shapes mirror the
 * runtime objects produced by `buildCallGraph` in call-graph.js.
 */
export type CallTarget =
  | InternalCallTarget
  | SyscallCallTarget
  | MethodTokenCallTarget
  | IndirectCallTarget;

export interface CallEdge {
  caller: MethodRef;
  callOffset: number;
  opcode: string;
  target: CallTarget;
}

export interface CallGraph {
  methods: MethodRef[];
  edges: CallEdge[];
}

export type ReturnBehavior = "value" | "void" | "unknown";

export interface MethodContract {
  method: MethodRef;
  argumentCount: number;
  returnBehavior: ReturnBehavior;
}

export interface MethodContracts {
  methods: MethodContract[];
}

export interface SlotXref {
  index: number;
  reads: number[];
  writes: number[];
}

export interface MethodXrefs {
  method: MethodRef;
  locals: SlotXref[];
  arguments: SlotXref[];
  statics: SlotXref[];
}

export interface Xrefs {
  methods: MethodXrefs[];
}

export interface MethodTypes {
  method: MethodRef;
  arguments: string[];
  locals: string[];
}

export interface TypeInfo {
  methods: MethodTypes[];
  statics: string[];
}

export type PatternConfidence = "high" | "medium" | "low" | "unknown";

export interface PatternEvidence {
  source: string;
  value: string;
}

export interface PatternInfo {
  standards: string[];
  patterns: string[];
  language: string | null;
  compiler: string | null;
  confidence: PatternConfidence;
  evidence: PatternEvidence[];
}

// ─── Options ───────────────────────────────────────────────────────────────

export interface DisassemblyOptions {
  failOnUnknownOpcodes?: boolean;
}

export interface DecompileOptions extends DisassemblyOptions {
  /**
   * Use conservative inferred C# declaration types when rendering the
   * `csharp` result. Defaults to `false`, which preserves `var` declarations.
   */
  typedDeclarations?: boolean;
  /**
   * Inline single-use temporary variables (`tN`) into their first use site
   * for tighter, more readable output. Disabled by default to preserve
   * one-name-per-stack-slot output that's easier to step through.
   */
  inlineSingleUseTemps?: boolean;
  /**
   * Convenience shorthand for the maximum-readability mode: enables every
   * end-user-friendly postprocess option (currently `inlineSingleUseTemps`,
   * with future readability options auto-composing under the same flag).
   * Recommended when consuming the high-level output as source code.
   */
  clean?: boolean;
}

// ─── Result types ──────────────────────────────────────────────────────────

export interface DisassemblyResult {
  instructions: Instruction[];
  warnings: string[];
}

export interface DecompileResult {
  nef: NefFile;
  instructions: Instruction[];
  warnings: string[];
  pseudocode: string;
  /** Conservative standard, behavior-pattern, and language identification. */
  patterns: PatternInfo;
}

export interface DecompileWithManifestResult extends DecompileResult {
  manifest: ContractManifest;
  methodGroups: MethodGroup[];
  groupedPseudocode: string;
}

export interface HighLevelResult extends DecompileResult {
  methodGroups: MethodGroup[];
  methodContracts: MethodContracts;
  /** Conservative standard, behavior-pattern, and language identification. */
  patterns: PatternInfo;
  highLevel: string;
  /** Readable C#-style rendering of the high-level surface. */
  csharp: string;
}

export interface HighLevelWithManifestResult extends HighLevelResult {
  manifest: ContractManifest;
  groupedPseudocode: string;
}

export interface AnalyzeResult extends DecompileResult {
  manifest: ContractManifest | null;
  methodGroups: MethodGroup[];
  callGraph: CallGraph;
  methodContracts: MethodContracts;
  xrefs: Xrefs;
  types: TypeInfo;
  patterns: PatternInfo;
}

// ─── Public API functions ──────────────────────────────────────────────────

export function parseNef(bytes: Uint8Array | ArrayBuffer | number[]): NefFile;

export function disassembleScript(
  script: Uint8Array | ArrayBuffer | number[],
  options?: DisassemblyOptions,
): DisassemblyResult;

export function parseManifest(
  json: string | Record<string, unknown>,
  options?: { strict?: boolean },
): ContractManifest;

export function renderCSharpContract(
  highLevel: string,
  manifest?: ContractManifest | null,
  options?: { typedDeclarations?: boolean },
  patternInfo?: PatternInfo | null,
): string;

export function decompileBytes(
  bytes: Uint8Array | ArrayBuffer | number[],
  options?: DisassemblyOptions,
): DecompileResult;

export function decompileBytesWithManifest(
  bytes: Uint8Array | ArrayBuffer | number[],
  manifest: string | Record<string, unknown>,
  options?: DisassemblyOptions,
): DecompileWithManifestResult;

export function decompileHighLevelBytes(
  bytes: Uint8Array | ArrayBuffer | number[],
  options?: DecompileOptions,
): HighLevelResult;

export function decompileHighLevelBytesWithManifest(
  bytes: Uint8Array | ArrayBuffer | number[],
  manifest: string | Record<string, unknown>,
  options?: DecompileOptions,
): HighLevelWithManifestResult;

export function analyzeBytes(
  bytes: Uint8Array | ArrayBuffer | number[],
  manifest?: string | Record<string, unknown> | null,
  options?: DisassemblyOptions,
): AnalyzeResult;

export function buildCallGraph(
  nef: NefFile,
  instructions: Instruction[],
  methodGroups: MethodGroup[],
): CallGraph;

export function buildMethodGroups(
  instructions: Instruction[],
  manifest: ContractManifest | null,
): MethodGroup[];

export function buildXrefs(
  instructions: Instruction[],
  methodGroups: MethodGroup[],
): Xrefs;

export function inferTypes(
  instructions: Instruction[],
  methodGroups: MethodGroup[],
  manifest?: ContractManifest | null,
): TypeInfo;

export function identifyPatterns(
  nef: NefFile,
  instructions: Instruction[],
  manifest?: ContractManifest | null,
): PatternInfo;

export function renderPseudocode(instructions: Instruction[]): string;

export function renderGroupedPseudocode(
  groups: MethodGroup[],
  manifest: ContractManifest | null,
): string;

export function renderHighLevelMethodGroups(
  groups: MethodGroup[],
  manifest: ContractManifest | null,
  context?: unknown,
): string;

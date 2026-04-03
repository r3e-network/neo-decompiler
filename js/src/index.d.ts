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
  scriptHash: string;
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

export interface ManifestFeatures {
  storage: boolean;
  payable: boolean;
}

export interface ContractManifest {
  name: string;
  groups: unknown[];
  supportedStandards: string[];
  features: ManifestFeatures;
  abi: ManifestAbi;
  permissions: unknown[];
  trusts: unknown[] | null;
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

export interface CallTarget {
  kind: "Internal" | "MethodToken" | "Syscall" | "Indirect";
  method?: MethodRef;
  hashLe?: string;
  hashBe?: string;
  name?: string;
  tokenMethod?: string;
  parametersCount?: number;
  hasReturnValue?: boolean;
  callFlags?: number;
}

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

// ─── Options ───────────────────────────────────────────────────────────────

export interface DisassemblyOptions {
  failOnUnknownOpcodes?: boolean;
}

export interface DecompileOptions extends DisassemblyOptions {
  inlineSingleUseTemps?: boolean;
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
}

export interface DecompileWithManifestResult extends DecompileResult {
  manifest: ContractManifest;
  methodGroups: MethodGroup[];
  groupedPseudocode: string;
}

export interface HighLevelResult extends DecompileResult {
  methodGroups: MethodGroup[];
  highLevel: string;
}

export interface HighLevelWithManifestResult extends HighLevelResult {
  manifest: ContractManifest;
  groupedPseudocode: string;
}

export interface AnalyzeResult extends DecompileResult {
  manifest: ContractManifest | null;
  methodGroups: MethodGroup[];
  callGraph: CallGraph;
  xrefs: Xrefs;
  types: TypeInfo;
}

// ─── Public API functions ──────────────────────────────────────────────────

export function parseNef(bytes: Uint8Array | ArrayBuffer | number[]): NefFile;

export function disassembleScript(
  script: Uint8Array | ArrayBuffer | number[],
  options?: DisassemblyOptions,
): DisassemblyResult;

export function parseManifest(json: string | Record<string, unknown>): ContractManifest;

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

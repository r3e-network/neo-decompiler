export type WasmInitInput =
  | RequestInfo
  | URL
  | Response
  | BufferSource
  | WebAssembly.Module;

export type OutputFormat = "all" | "pseudocode" | "highLevel" | "csharp";

export interface InfoOptions {
  manifestJson?: string;
  strictManifest?: boolean;
}

export interface DisasmOptions {
  failOnUnknownOpcodes?: boolean;
}

export interface DecompileOptions {
  manifestJson?: string;
  strictManifest?: boolean;
  failOnUnknownOpcodes?: boolean;
  inlineSingleUseTemps?: boolean;
  outputFormat?: OutputFormat;
}

export interface NativeContractReport {
  contract: string;
  method?: string | null;
  label: string;
}

export interface MethodTokenReport {
  method: string;
  hash_le: string;
  hash_be: string;
  parameters: number;
  returns: boolean;
  call_flags: number;
  call_flag_labels: string[];
  returns_value: boolean;
  native_contract?: NativeContractReport | null;
  warning?: string | null;
}

export interface OperandValueReport {
  type:
    | "I8"
    | "I16"
    | "I32"
    | "I64"
    | "U8"
    | "U16"
    | "U32"
    | "Bool"
    | "Bytes"
    | "Jump"
    | "Jump32"
    | "Syscall"
    | "Null";
  value?: number | string | boolean;
}

export interface InstructionReport {
  offset: number;
  opcode: string;
  operand?: string | null;
  operand_kind?: string | null;
  operand_value?: OperandValueReport | null;
  returns_value?: boolean | null;
}

export interface GroupSummary {
  pubkey: string;
  signature: string;
}

export type PermissionContractSummary =
  | { type: "Wildcard"; value: string }
  | { type: "Hash"; value: string }
  | { type: "Group"; value: string }
  | { type: "Other"; value: unknown };

export type PermissionMethodsSummary =
  | { type: "Wildcard"; value: string }
  | { type: "Methods"; value: string[] };

export type TrustSummary =
  | { type: "Wildcard"; value: string }
  | { type: "Contracts"; value: string[] }
  | { type: "Other"; value: unknown };

export interface ParameterSummary {
  name: string;
  ty: string;
}

export interface MethodSummary {
  name: string;
  parameters: ParameterSummary[];
  return_type: string;
  safe: boolean;
  offset?: number | null;
}

export interface EventSummary {
  name: string;
  parameters: ParameterSummary[];
}

export interface AbiSummary {
  methods: MethodSummary[];
  events: EventSummary[];
}

export interface PermissionSummary {
  contract: PermissionContractSummary;
  methods: PermissionMethodsSummary;
}

export interface ManifestSummary {
  name: string;
  supported_standards: string[];
  storage: boolean;
  payable: boolean;
  groups: GroupSummary[];
  methods: number;
  events: number;
  permissions: PermissionSummary[];
  trusts?: TrustSummary | null;
  abi: AbiSummary;
}

export interface MethodRef {
  offset: number;
  name: string;
}

export type CallTarget =
  | { Internal: { method: MethodRef } }
  | {
      MethodToken: {
        index: number;
        hash_le: string;
        hash_be: string;
        method: string;
        parameters_count: number;
        has_return_value: boolean;
        call_flags: number;
      };
    }
  | { Syscall: { hash: number; name?: string | null; returns_value: boolean } }
  | { Indirect: { opcode: string; operand?: number | null } }
  | { UnresolvedInternal: { target: number } };

export interface CallEdge {
  caller: MethodRef;
  call_offset: number;
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

export type ValueType =
  | "unknown"
  | "any"
  | "null"
  | "bool"
  | "integer"
  | "bytestring"
  | "buffer"
  | "array"
  | "struct"
  | "map"
  | "interopinterface"
  | "pointer";

export interface MethodTypes {
  method: MethodRef;
  arguments: ValueType[];
  locals: ValueType[];
}

export interface TypeInfo {
  methods: MethodTypes[];
  statics: ValueType[];
}

export interface AnalysisReport {
  call_graph: CallGraph;
  xrefs: Xrefs;
  types: TypeInfo;
}

export interface WebInfoReport {
  compiler: string;
  source?: string | null;
  script_length: number;
  script_hash_le: string;
  script_hash_be: string;
  checksum: string;
  method_tokens: MethodTokenReport[];
  manifest?: ManifestSummary | null;
  warnings: string[];
}

export interface WebDisasmReport {
  instructions: InstructionReport[];
  warnings: string[];
}

export interface WebDecompileReport {
  script_hash_le: string;
  script_hash_be: string;
  csharp: string;
  high_level: string;
  pseudocode: string;
  instructions: InstructionReport[];
  method_tokens: MethodTokenReport[];
  manifest?: ManifestSummary | null;
  analysis: AnalysisReport;
  warnings: string[];
}

export interface WasmBindings {
  infoReport(nefBytes: Uint8Array, options?: unknown): WebInfoReport;
  disasmReport(nefBytes: Uint8Array, options?: unknown): WebDisasmReport;
  decompileReport(nefBytes: Uint8Array, options?: unknown): WebDecompileReport;
  initPanicHook(): void;
}

interface WasmModule extends WasmBindings {
  default(input?: WasmInitInput): Promise<unknown>;
}

export interface NeoDecompilerClient {
  infoReport(nefBytes: Uint8Array, options?: InfoOptions): WebInfoReport;
  disasmReport(nefBytes: Uint8Array, options?: DisasmOptions): WebDisasmReport;
  decompileReport(
    nefBytes: Uint8Array,
    options?: DecompileOptions,
  ): WebDecompileReport;
  initPanicHook(): void;
}

export function createNeoDecompilerClient(
  bindings: WasmBindings,
): NeoDecompilerClient {
  return {
    infoReport(nefBytes, options) {
      return bindings.infoReport(nefBytes, normalizeInfoOptions(options));
    },
    disasmReport(nefBytes, options) {
      return bindings.disasmReport(nefBytes, normalizeDisasmOptions(options));
    },
    decompileReport(nefBytes, options) {
      return bindings.decompileReport(
        nefBytes,
        normalizeDecompileOptions(options),
      );
    },
    initPanicHook() {
      bindings.initPanicHook();
    },
  };
}

let defaultClient: NeoDecompilerClient | null = null;
const WASM_MODULE_PATH = "./pkg/neo_decompiler.js";

export async function init(
  input?: WasmInitInput,
): Promise<NeoDecompilerClient> {
  const wasm = (await import(WASM_MODULE_PATH)) as WasmModule;
  await wasm.default(input);
  defaultClient = createNeoDecompilerClient({
    infoReport: wasm.infoReport,
    disasmReport: wasm.disasmReport,
    decompileReport: wasm.decompileReport,
    initPanicHook: wasm.initPanicHook,
  });
  return defaultClient;
}

export function infoReport(
  nefBytes: Uint8Array,
  options?: InfoOptions,
): WebInfoReport {
  return requireClient().infoReport(nefBytes, options);
}

export function disasmReport(
  nefBytes: Uint8Array,
  options?: DisasmOptions,
): WebDisasmReport {
  return requireClient().disasmReport(nefBytes, options);
}

export function decompileReport(
  nefBytes: Uint8Array,
  options?: DecompileOptions,
): WebDecompileReport {
  return requireClient().decompileReport(nefBytes, options);
}

export function initPanicHook(): void {
  requireClient().initPanicHook();
}

function requireClient(): NeoDecompilerClient {
  if (defaultClient === null) {
    throw new Error(
      "neo-decompiler-web is not initialized; call init() before using report functions",
    );
  }
  return defaultClient;
}

function normalizeInfoOptions(options: InfoOptions = {}): Record<string, unknown> {
  const normalized: Record<string, unknown> = {};
  if (options.manifestJson !== undefined) {
    normalized.manifest_json = options.manifestJson;
  }
  if (options.strictManifest !== undefined) {
    normalized.strict_manifest = options.strictManifest;
  }
  return normalized;
}

function normalizeDisasmOptions(
  options: DisasmOptions = {},
): Record<string, unknown> {
  const normalized: Record<string, unknown> = {};
  if (options.failOnUnknownOpcodes !== undefined) {
    normalized.fail_on_unknown_opcodes = options.failOnUnknownOpcodes;
  }
  return normalized;
}

function normalizeDecompileOptions(
  options: DecompileOptions = {},
): Record<string, unknown> {
  const normalized = normalizeInfoOptions(options);
  if (options.failOnUnknownOpcodes !== undefined) {
    normalized.fail_on_unknown_opcodes = options.failOnUnknownOpcodes;
  }
  if (options.inlineSingleUseTemps !== undefined) {
    normalized.inline_single_use_temps = options.inlineSingleUseTemps;
  }
  if (options.outputFormat !== undefined) {
    normalized.output_format = options.outputFormat;
  }
  return normalized;
}

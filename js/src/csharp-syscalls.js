import { SYSCALLS } from "./generated/syscalls.js";

const CSHARP_SYSCALLS = new Map([
  ["System.Contract.CreateStandardAccount", "Contract.CreateStandardAccount"],
  ["System.Contract.CreateMultisigAccount", "Contract.CreateMultisigAccount"],
  ["System.Storage.GetContext", "Storage.CurrentContext"],
  ["System.Storage.GetReadOnlyContext", "Storage.CurrentReadOnlyContext"],
  ["System.Runtime.GetTime", "Runtime.Time"],
  ["System.Runtime.GetRandom", "Runtime.GetRandom"],
  ["System.Runtime.GetScriptContainer", "Runtime.Transaction"],
  ["System.Runtime.GetCallingScriptHash", "Runtime.CallingScriptHash"],
  ["System.Runtime.GetEntryScriptHash", "Runtime.EntryScriptHash"],
  ["System.Runtime.GetExecutingScriptHash", "Runtime.ExecutingScriptHash"],
  ["System.Runtime.GetInvocationCounter", "Runtime.InvocationCounter"],
  ["System.Contract.GetCallFlags", "Contract.GetCallFlags"],
  ["System.Runtime.GetNetwork", "Runtime.GetNetwork"],
  ["System.Runtime.GetTrigger", "Runtime.Trigger"],
  ["System.Runtime.CurrentSigners", "Runtime.CurrentSigners"],
  ["System.Runtime.GasLeft", "Runtime.GasLeft"],
  ["System.Runtime.GetAddressVersion", "Runtime.AddressVersion"],
  ["System.Runtime.Platform", "Runtime.Platform"],
  ["System.Crypto.CheckSig", "Crypto.CheckSig"],
  ["System.Crypto.CheckMultisig", "Crypto.CheckMultisig"],
  ["System.Storage.Get", "Storage.Get"],
  ["System.Storage.Put", "Storage.Put"],
  ["System.Storage.Delete", "Storage.Delete"],
  ["System.Storage.Find", "Storage.Find"],
  ["System.Runtime.Log", "Runtime.Log"],
  ["System.Runtime.CheckWitness", "Runtime.CheckWitness"],
  ["System.Runtime.GetNotifications", "Runtime.GetNotifications"],
  ["System.Runtime.BurnGas", "Runtime.BurnGas"],
  ["System.Runtime.LoadScript", "Runtime.LoadScript"],
  ["System.Contract.Call", "Contract.Call"],
]);

// Return types used by the C# body type planner. These follow the VM-oriented
// types emitted by the Rust renderer so numeric results remain composable as
// BigInteger expressions and collection results stay framework-compatible.
const CSHARP_SYSCALL_RETURN_TYPES = new Map([
  ["System.Contract.CreateStandardAccount", "UInt160"],
  ["System.Contract.CreateMultisigAccount", "UInt160"],
  ["System.Crypto.CheckSig", "bool"],
  ["System.Crypto.CheckMultisig", "bool"],
  ["System.Iterator.Next", "bool"],
  ["System.Runtime.CheckWitness", "bool"],
  ["System.Runtime.GetAddressVersion", "BigInteger"],
  ["System.Runtime.GetInvocationCounter", "BigInteger"],
  ["System.Runtime.GetNetwork", "BigInteger"],
  ["System.Runtime.GetRandom", "BigInteger"],
  ["System.Runtime.GetTime", "BigInteger"],
  ["System.Runtime.GasLeft", "BigInteger"],
  ["System.Runtime.GetCallingScriptHash", "UInt160"],
  ["System.Runtime.GetEntryScriptHash", "UInt160"],
  ["System.Runtime.GetExecutingScriptHash", "UInt160"],
  ["System.Runtime.GetNotifications", "object[]"],
  ["System.Runtime.CurrentSigners", "object[]"],
  ["System.Runtime.GetScriptContainer", "Transaction"],
  ["System.Runtime.Platform", "string"],
  ["System.Storage.Get", "ByteString"],
  ["System.Storage.Local.Get", "ByteString"],
  ["System.Storage.GetContext", "StorageContext"],
  ["System.Storage.GetReadOnlyContext", "StorageContext"],
  ["System.Storage.AsReadOnly", "StorageContext"],
  ["System.Storage.Find", "Iterator"],
  ["System.Storage.Local.Find", "Iterator"],
]);

// These syscalls are exposed by Neo's VM, but do not have public
// SmartContract.Framework methods. Keep them representable in valid C# by
// replaying the syscall through Runtime.LoadScript, matching the Rust
// renderer's low-level fallback.
const CSHARP_LOW_LEVEL_SYSCALLS = new Set([
  "System.Contract.CallNative",
  "System.Contract.NativeOnPersist",
  "System.Contract.NativePostPersist",
  "System.Runtime.Notify",
]);

const SYSCALL_HASHES_BY_NAME = new Map(
  [...SYSCALLS.values()].map((info) => [info.name, info.hash]),
);

const STATIC_SYSCALLS = new Set([
  "System.Storage.GetContext",
  "System.Storage.GetReadOnlyContext",
  "System.Runtime.GetTime",
  "System.Runtime.GetCallingScriptHash",
  "System.Runtime.GetEntryScriptHash",
  "System.Runtime.GetExecutingScriptHash",
  "System.Runtime.GetInvocationCounter",
  "System.Runtime.GetTrigger",
  "System.Runtime.GetScriptContainer",
  "System.Runtime.GasLeft",
  "System.Runtime.GetAddressVersion",
  "System.Runtime.Platform",
]);

export function csharpSyscallReturnType(name) {
  return CSHARP_SYSCALL_RETURN_TYPES.get(name) ?? null;
}

export function renderCSharpSyscall(name, args) {
  const iteratorMethod = {
    "System.Iterator.Next": (receiver) => `${receiver}.Next()`,
    "System.Iterator.Value": (receiver) => `${receiver}.Value`,
    "System.Storage.AsReadOnly": (receiver) => `${receiver}.AsReadOnly`,
  }[name];
  if (iteratorMethod && args.length === 1) return iteratorMethod(args[0]);

  const localStorageMethod = {
    "System.Storage.Local.Get": "Get",
    "System.Storage.Local.Put": "Put",
    "System.Storage.Local.Delete": "Delete",
    "System.Storage.Local.Find": "Find",
  }[name];
  if (localStorageMethod) return renderStorageCall(localStorageMethod, args, true);

  if (CSHARP_LOW_LEVEL_SYSCALLS.has(name)) return renderLowLevelSyscall(name, args);

  const api = CSHARP_SYSCALLS.get(name);
  if (!api) {
    const hash = parseNumericSyscallHash(name);
    return hash === null
      ? renderUnresolvedSyscall(name, args)
      : renderLowLevelSyscallHash(hash, args);
  }
  const storageMethod = api.startsWith("Storage.") ? api.slice("Storage.".length) : null;
  if (storageMethod && ["Get", "Put", "Delete", "Find"].includes(storageMethod)) {
    return renderStorageCall(storageMethod, args, false);
  }
  if (name === "System.Contract.Call" && args[2] !== undefined) {
    const rendered = [...args];
    rendered[2] = renderNumericEnum(rendered[2], "CallFlags");
    return `Contract.Call(${rendered.join(", ")})`;
  }
  return api.includes(".") && args.length === 0 && STATIC_SYSCALLS.has(name)
    ? api
    : `${api}(${args.join(", ")})`;
}

function renderStorageCall(method, args, local) {
  const rendered = [...args];
  const keyIndex = local ? 0 : 1;
  if (rendered[keyIndex] !== undefined) {
    rendered[keyIndex] = renderNumericStorageKey(rendered[keyIndex]);
  }
  if (method === "Find") {
    const optionsIndex = local ? 1 : 2;
    if (rendered[optionsIndex] !== undefined) {
      rendered[optionsIndex] = renderNumericFindOptions(rendered[optionsIndex]);
    }
  }
  return `Storage.${method}(${rendered.join(", ")})`;
}

function renderNumericStorageKey(expression) {
  const source = expression.trim();
  return /^-?(?:0x[0-9a-f]+|[0-9]+)$/i.test(source)
    ? `(ByteString)(BigInteger)(${source})`
    : expression;
}

function renderNumericFindOptions(expression) {
  return renderNumericEnum(expression, "FindOptions");
}

function renderNumericEnum(expression, type) {
  const source = expression.trim();
  return /^-?(?:0x[0-9a-f]+|[0-9]+)$/i.test(source)
    ? `(${type})(${source})`
    : expression;
}

function renderLowLevelSyscall(name, args) {
  const hash = SYSCALL_HASHES_BY_NAME.get(name);
  return hash === undefined ? null : renderLowLevelSyscallHash(hash, args);
}

function renderLowLevelSyscallHash(hash, args) {
  const value = Number(hash) >>> 0;
  const bytes = [
    0x41,
    value & 0xff,
    (value >>> 8) & 0xff,
    (value >>> 16) & 0xff,
    (value >>> 24) & 0xff,
  ].map((byte) => `0x${byte.toString(16).padStart(2, "0").toUpperCase()}`);
  return `Runtime.LoadScript((ByteString)new byte[] { ${bytes.join(", ")} }, CallFlags.All, new object[] { ${args.join(", ")} })`;
}

function parseNumericSyscallHash(name) {
  const match = String(name).match(/^0x([0-9a-fA-F]{1,8})$/);
  return match ? Number.parseInt(match[1], 16) >>> 0 : null;
}

function renderUnresolvedSyscall(name, args) {
  const label = String(name)
    .replace(/\\/g, "\\\\")
    .replace(/"/g, '\\"')
    .replace(/\*\//g, "* /")
    .replace(/[\r\n]/g, " ");
  const renderedArgs = args
    .join(", ")
    .replace(/\*\//g, "* /")
    .replace(/[\r\n]/g, " ");
  return `default(dynamic) /* unresolved VM syscall \"${label}\"(${renderedArgs}) */`;
}

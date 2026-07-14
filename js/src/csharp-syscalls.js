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
  if (localStorageMethod) return `Storage.${localStorageMethod}(${args.join(", ")})`;

  if (CSHARP_LOW_LEVEL_SYSCALLS.has(name)) return renderLowLevelSyscall(name, args);

  const api = CSHARP_SYSCALLS.get(name);
  if (!api) return null;
  return api.includes(".") && args.length === 0 && STATIC_SYSCALLS.has(name)
    ? api
    : `${api}(${args.join(", ")})`;
}

function renderLowLevelSyscall(name, args) {
  const hash = SYSCALL_HASHES_BY_NAME.get(name);
  if (hash === undefined) return null;
  const bytes = [
    0x41,
    hash & 0xff,
    (hash >>> 8) & 0xff,
    (hash >>> 16) & 0xff,
    (hash >>> 24) & 0xff,
  ].map((byte) => `0x${byte.toString(16).padStart(2, "0").toUpperCase()}`);
  return `Runtime.LoadScript((ByteString)new byte[] { ${bytes.join(", ")} }, CallFlags.All, new object[] { ${args.join(", ")} })`;
}

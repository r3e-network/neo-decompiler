export function inferSyscallPatterns(name, patterns, evidence) {
  if (name.startsWith("System.Storage.")) add(patterns, evidence, "storage", name);
  if (name === "System.Storage.Get" || name === "System.Storage.Local.Get") {
    add(patterns, evidence, "storage_reads", name);
  }
  if (name === "System.Storage.Put" || name === "System.Storage.Local.Put") {
    add(patterns, evidence, "storage_writes", name);
  }
  if (name === "System.Storage.Delete" || name === "System.Storage.Local.Delete") {
    add(patterns, evidence, "storage_deletes", name);
  }
  if (name === "System.Storage.Find" || name === "System.Storage.Local.Find") {
    add(patterns, evidence, "storage_iteration", name);
  }
  if (name === "System.Iterator.Next" || name === "System.Iterator.Value") {
    add(patterns, evidence, "iterator_usage", name);
  }
  if (name === "System.Runtime.Notify" || name === "System.Runtime.Log") {
    add(patterns, evidence, "notifications", name);
  }
  if (name === "System.Crypto.CheckSig" || name === "System.Crypto.CheckMultisig") {
    patterns.add("signature_verification");
    if (name === "System.Crypto.CheckMultisig") patterns.add("multisig");
    evidence.push({ source: "syscall", value: name });
  }
  if (name === "System.Runtime.CheckWitness") {
    add(patterns, evidence, "authorization", name);
  }
  if (name === "System.Runtime.GetCallingScriptHash") {
    add(patterns, evidence, "caller_context", name);
  }
  if (name === "System.Runtime.CurrentSigners") {
    add(patterns, evidence, "signer_introspection", name);
  }
  if ([
    "System.Runtime.GetAddressVersion",
    "System.Runtime.GetEntryScriptHash",
    "System.Runtime.GetExecutingScriptHash",
    "System.Runtime.GetInvocationCounter",
    "System.Runtime.GetNetwork",
    "System.Runtime.GetNotifications",
    "System.Runtime.GetRandom",
    "System.Runtime.GetScriptContainer",
    "System.Runtime.GetTime",
    "System.Runtime.GetTrigger",
    "System.Runtime.Platform",
  ].includes(name)) {
    add(patterns, evidence, "runtime_context", name);
  }
  if (
    name === "System.Contract.CreateMultisigAccount" ||
    name === "System.Contract.CreateStandardAccount"
  ) {
    add(patterns, evidence, "account_creation", name);
  }
  if (name === "System.Runtime.BurnGas" || name === "System.Runtime.GasLeft") {
    add(patterns, evidence, "gas_management", name);
  }
}

function add(patterns, evidence, pattern, syscall) {
  patterns.add(pattern);
  evidence.push({ source: "syscall", value: syscall });
}

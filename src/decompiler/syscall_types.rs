//! Return types for syscall-backed Neo C# framework APIs.

use crate::decompiler::analysis::types::ValueType;
use crate::syscalls;

/// A known syscall return type in VM and generated C# forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SyscallReturnType {
    pub(crate) value_type: ValueType,
    pub(crate) csharp_type: &'static str,
}

/// Resolve a syscall's stable framework return type from the syscall catalog.
///
/// Catalog lookup is required so an arbitrary hash cannot acquire a framework
/// type merely by matching a familiar rendered API name.
pub(crate) fn lookup(hash: u32) -> Option<SyscallReturnType> {
    let name = syscalls::lookup(hash)?.name;
    match name {
        "System.Contract.CreateStandardAccount" | "System.Contract.CreateMultisigAccount" => {
            Some(return_type(ValueType::ByteString, "UInt160"))
        }
        "System.Contract.Call" | "System.Contract.CallNative" => {
            Some(return_type(ValueType::Unknown, "object"))
        }
        "System.Crypto.CheckSig" | "System.Crypto.CheckMultisig" => {
            Some(return_type(ValueType::Boolean, "bool"))
        }
        "System.Contract.GetCallFlags" => Some(return_type(ValueType::Integer, "CallFlags")),
        "System.Iterator.Next" => Some(return_type(ValueType::Boolean, "bool")),
        "System.Iterator.Value" => Some(return_type(ValueType::Unknown, "object")),
        "System.Runtime.CheckWitness" => Some(return_type(ValueType::Boolean, "bool")),
        "System.Runtime.GetAddressVersion" => Some(return_type(ValueType::Integer, "byte")),
        "System.Runtime.GetInvocationCounter" => Some(return_type(ValueType::Integer, "uint")),
        "System.Runtime.GetNetwork" => Some(return_type(ValueType::Integer, "uint")),
        "System.Runtime.GetRandom" => Some(return_type(ValueType::Integer, "BigInteger")),
        "System.Runtime.GetTime" => Some(return_type(ValueType::Integer, "ulong")),
        "System.Runtime.GasLeft" => Some(return_type(ValueType::Integer, "long")),
        "System.Runtime.GetCallingScriptHash"
        | "System.Runtime.GetEntryScriptHash"
        | "System.Runtime.GetExecutingScriptHash" => {
            Some(return_type(ValueType::ByteString, "UInt160"))
        }
        "System.Runtime.GetNotifications" => Some(return_type(ValueType::Array, "Notification[]")),
        "System.Runtime.CurrentSigners" => Some(return_type(ValueType::Array, "Signer[]")),
        "System.Runtime.LoadScript" => Some(return_type(ValueType::Unknown, "object")),
        "System.Runtime.GetTrigger" => Some(return_type(ValueType::Integer, "TriggerType")),
        "System.Runtime.GetScriptContainer" => {
            Some(return_type(ValueType::InteropInterface, "Transaction"))
        }
        "System.Runtime.Platform" => Some(return_type(ValueType::ByteString, "string")),
        "System.Storage.Get" | "System.Storage.Local.Get" => {
            Some(return_type(ValueType::ByteString, "ByteString"))
        }
        "System.Storage.GetContext"
        | "System.Storage.GetReadOnlyContext"
        | "System.Storage.AsReadOnly" => {
            Some(return_type(ValueType::InteropInterface, "StorageContext"))
        }
        "System.Storage.Find" | "System.Storage.Local.Find" => {
            Some(return_type(ValueType::InteropInterface, "Iterator"))
        }
        _ => None,
    }
}

fn return_type(value_type: ValueType, csharp_type: &'static str) -> SyscallReturnType {
    SyscallReturnType {
        value_type,
        csharp_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_catalog_bound_framework_returns() {
        let witness = lookup(0x8CEC_27F8).expect("CheckWitness syscall");
        assert_eq!(witness.value_type, ValueType::Boolean);
        assert_eq!(witness.csharp_type, "bool");

        let storage = lookup(0x31E8_5D92).expect("Storage.Get syscall");
        assert_eq!(storage.value_type, ValueType::ByteString);
        assert_eq!(storage.csharp_type, "ByteString");

        let iterator = lookup(0x9AB8_30DF).expect("Storage.Find syscall");
        assert_eq!(iterator.value_type, ValueType::InteropInterface);
        assert_eq!(iterator.csharp_type, "Iterator");

        let local_iterator = lookup(0xF352_7607).expect("Storage.Local.Find syscall");
        assert_eq!(local_iterator.value_type, ValueType::InteropInterface);
        assert_eq!(local_iterator.csharp_type, "Iterator");
    }

    #[test]
    fn preserves_framework_scalar_and_collection_return_types() {
        let time = lookup(0x0388_C3B7).expect("Runtime.Time syscall");
        assert_eq!(time.value_type, ValueType::Integer);
        assert_eq!(time.csharp_type, "ulong");

        let flags = lookup(0x813A_DA95).expect("Contract.GetCallFlags syscall");
        assert_eq!(flags.value_type, ValueType::Integer);
        assert_eq!(flags.csharp_type, "CallFlags");

        let notifications = lookup(0xF135_4327).expect("Runtime.GetNotifications syscall");
        assert_eq!(notifications.value_type, ValueType::Array);
        assert_eq!(notifications.csharp_type, "Notification[]");

        let signers = lookup(0x8B18_F1AC).expect("Runtime.CurrentSigners syscall");
        assert_eq!(signers.value_type, ValueType::Array);
        assert_eq!(signers.csharp_type, "Signer[]");

        let trigger = lookup(0xA038_7DE9).expect("Runtime.GetTrigger syscall");
        assert_eq!(trigger.value_type, ValueType::Integer);
        assert_eq!(trigger.csharp_type, "TriggerType");

        let object = lookup(0x677B_F71A).expect("Contract.CallNative syscall");
        assert_eq!(object.value_type, ValueType::Unknown);
        assert_eq!(object.csharp_type, "object");
    }

    #[test]
    fn unknown_hashes_remain_untyped() {
        assert!(lookup(0xDEAD_BEEF).is_none());
    }
}

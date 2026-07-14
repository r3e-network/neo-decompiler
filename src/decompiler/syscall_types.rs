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
        "System.Crypto.CheckSig" | "System.Crypto.CheckMultisig" => {
            Some(return_type(ValueType::Boolean, "bool"))
        }
        "System.Iterator.Next" => Some(return_type(ValueType::Boolean, "bool")),
        "System.Runtime.CheckWitness" => Some(return_type(ValueType::Boolean, "bool")),
        "System.Runtime.GetAddressVersion"
        | "System.Runtime.GetInvocationCounter"
        | "System.Runtime.GetNetwork"
        | "System.Runtime.GetRandom"
        | "System.Runtime.GetTime"
        | "System.Runtime.GasLeft" => Some(return_type(ValueType::Integer, "BigInteger")),
        "System.Runtime.GetCallingScriptHash"
        | "System.Runtime.GetEntryScriptHash"
        | "System.Runtime.GetExecutingScriptHash" => {
            Some(return_type(ValueType::ByteString, "UInt160"))
        }
        "System.Runtime.GetNotifications" | "System.Runtime.CurrentSigners" => {
            Some(return_type(ValueType::Array, "object[]"))
        }
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
    fn unknown_hashes_remain_untyped() {
        assert!(lookup(0xDEAD_BEEF).is_none());
    }
}

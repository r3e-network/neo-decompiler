//! Return types for native method tokens with stable Neo C# framework APIs.

use crate::decompiler::analysis::types::ValueType;
use crate::native_contracts;

/// A native method return type in both the VM-oriented and C#-oriented forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeMethodReturnType {
    pub(crate) value_type: ValueType,
    pub(crate) csharp_type: &'static str,
}

/// Resolve a known native method token to its stable C# return type.
///
/// The hash and method name are both required. A method name alone is not
/// enough to establish a native API contract because arbitrary external
/// contracts may expose the same name. Restricted calls remain dynamic so
/// their `Contract.Call` fallback does not acquire an unchecked static type.
pub(crate) fn lookup(
    hash_le: Option<&str>,
    method: &str,
    call_flags: Option<u8>,
) -> Option<NativeMethodReturnType> {
    if call_flags != Some(0x0F) {
        return None;
    }
    let hash = parse_hash(hash_le?)?;
    let hint = native_contracts::describe_method_token(&hash, method)?;
    let method = hint.canonical_method?;

    match (hint.contract, method) {
        ("ContractManagement", "Deploy" | "GetContract" | "GetContractById") => {
            Some(return_type(ValueType::InteropInterface, "Contract"))
        }
        ("ContractManagement", "GetContractHashes") => Some(return_type(
            ValueType::InteropInterface,
            "Iterator<(int, UInt160)>",
        )),
        ("ContractManagement", "HasMethod" | "IsContract") => {
            Some(return_type(ValueType::Boolean, "bool"))
        }
        ("ContractManagement", "GetMinimumDeploymentFee") => {
            Some(return_type(ValueType::Integer, "long"))
        }
        (
            "CryptoLib",
            "Bls12381Deserialize" | "Bls12381Add" | "Bls12381Mul" | "Bls12381Pairing",
        ) => Some(return_type(ValueType::Unknown, "object")),
        ("CryptoLib", "Bls12381Equal") => Some(return_type(ValueType::Boolean, "bool")),
        ("CryptoLib", "recoverSecp256K1") => Some(return_type(ValueType::ByteString, "ByteString")),
        ("CryptoLib", "Keccak256" | "Murmur32" | "Sha256" | "ripemd160") => {
            Some(return_type(ValueType::ByteString, "ByteString"))
        }
        ("CryptoLib", "VerifyWithECDsa" | "VerifyWithEd25519" | "verifyWithECDsa") => {
            Some(return_type(ValueType::Boolean, "bool"))
        }
        ("CryptoLib", "Bls12381Serialize") => Some(return_type(ValueType::Buffer, "byte[]")),
        ("LedgerContract", "CurrentHash") => Some(return_type(ValueType::ByteString, "UInt256")),
        ("LedgerContract", "CurrentIndex") => Some(return_type(ValueType::Integer, "uint")),
        ("LedgerContract", "GetBlock") => Some(return_type(ValueType::InteropInterface, "Block")),
        ("LedgerContract", "getTransaction" | "GetTransaction") => {
            Some(return_type(ValueType::InteropInterface, "Transaction"))
        }
        ("LedgerContract", "GetTransactionFromBlock") => {
            Some(return_type(ValueType::InteropInterface, "Transaction"))
        }
        ("LedgerContract", "GetTransactionHeight") => Some(return_type(ValueType::Integer, "int")),
        ("LedgerContract", "GetTransactionVMState") => {
            Some(return_type(ValueType::Integer, "VMState"))
        }
        ("LedgerContract", "GetTransactionSigners") => {
            Some(return_type(ValueType::Array, "Signer[]"))
        }
        ("GasToken" | "NeoToken", "Symbol") => Some(return_type(ValueType::ByteString, "string")),
        ("GasToken" | "NeoToken", "Decimals") => Some(return_type(ValueType::Integer, "byte")),
        (
            "GasToken" | "NeoToken",
            "BalanceOf" | "GetGasPerBlock" | "TotalSupply" | "UnclaimedGas",
        ) => Some(return_type(ValueType::Integer, "BigInteger")),
        ("GasToken" | "NeoToken", "Transfer") => Some(return_type(ValueType::Boolean, "bool")),
        ("NeoToken", "GetRegisterPrice") => Some(return_type(ValueType::Integer, "long")),
        ("NeoToken", "RegisterCandidate" | "UnregisterCandidate" | "Vote") => {
            Some(return_type(ValueType::Boolean, "bool"))
        }
        ("Notary", "BalanceOf") => Some(return_type(ValueType::Integer, "BigInteger")),
        ("Notary", "ExpirationOf" | "GetMaxNotValidBeforeDelta") => {
            Some(return_type(ValueType::Integer, "uint"))
        }
        ("Notary", "LockDepositUntil" | "Verify" | "Withdraw") => {
            Some(return_type(ValueType::Boolean, "bool"))
        }
        ("OracleContract", "GetPrice") => Some(return_type(ValueType::Integer, "long")),
        (
            "PolicyContract",
            "GetAttributeFee" | "getAttributeFee" | "GetExecFeeFactor" | "GetStoragePrice",
        ) => Some(return_type(ValueType::Integer, "uint")),
        ("PolicyContract", "GetExecPicoFeeFactor") => {
            Some(return_type(ValueType::Integer, "BigInteger"))
        }
        ("PolicyContract", "GetFeePerByte") => Some(return_type(ValueType::Integer, "long")),
        ("PolicyContract", "IsBlocked") => Some(return_type(ValueType::Boolean, "bool")),
        ("PolicyContract", "GetBlockedAccounts" | "GetWhitelistFeeContracts") => {
            Some(return_type(ValueType::InteropInterface, "Iterator"))
        }
        ("Treasury", "Verify") => Some(return_type(ValueType::Boolean, "bool")),
        ("RoleManagement", "GetDesignatedByRole") => {
            Some(return_type(ValueType::Array, "ECPoint[]"))
        }
        ("NeoToken", "GetCandidates") => {
            Some(return_type(ValueType::Array, "(ECPoint, BigInteger)[]"))
        }
        ("NeoToken", "GetAllCandidates") => Some(return_type(
            ValueType::InteropInterface,
            "Iterator<(ECPoint, BigInteger)>",
        )),
        ("NeoToken", "GetCandidateVote") => Some(return_type(ValueType::Integer, "BigInteger")),
        ("NeoToken", "GetCommittee" | "GetNextBlockValidators") => {
            Some(return_type(ValueType::Array, "ECPoint[]"))
        }
        ("NeoToken", "GetCommitteeAddress") => Some(return_type(ValueType::ByteString, "UInt160")),
        ("NeoToken", "GetAccountState") => {
            Some(return_type(ValueType::InteropInterface, "NeoAccountState"))
        }
        // The VM represents strings as ByteStrings, while generated C# keeps
        // the framework's string spelling for direct helper return types.
        ("StdLib", "Atoi") => Some(return_type(ValueType::Integer, "BigInteger")),
        ("StdLib", "Deserialize" | "JsonDeserialize") => {
            Some(return_type(ValueType::Unknown, "object"))
        }
        ("StdLib", "Itoa") => Some(return_type(ValueType::ByteString, "string")),
        (
            "StdLib",
            "Base64Encode" | "Base64UrlEncode" | "Base58Encode" | "Base58CheckEncode" | "HexEncode",
        ) => Some(return_type(ValueType::ByteString, "string")),
        ("StdLib", "Base64Decode" | "Base58Decode" | "Base58CheckDecode" | "HexDecode") => {
            Some(return_type(ValueType::ByteString, "ByteString"))
        }
        ("StdLib", "Serialize") => Some(return_type(ValueType::ByteString, "ByteString")),
        ("StdLib", "JsonSerialize") => Some(return_type(ValueType::ByteString, "string")),
        ("StdLib", "MemoryCompare" | "MemorySearch" | "StrLen") => {
            Some(return_type(ValueType::Integer, "BigInteger"))
        }
        ("StdLib", "StringSplit") => Some(return_type(ValueType::Array, "object[]")),
        _ => None,
    }
}

/// Return whether a fully resolved native method produces a VM value.
///
/// `lookup` intentionally only describes value-producing methods. Statement
/// rendering still needs to distinguish a known framework `void` method from
/// an unresolved token so it can avoid assigning the result of a void call.
pub(crate) fn returns_value(
    hash_le: Option<&str>,
    method: &str,
    call_flags: Option<u8>,
) -> Option<bool> {
    if call_flags != Some(0x0F) {
        return None;
    }
    let hash = parse_hash(hash_le?)?;
    let hint = native_contracts::describe_method_token(&hash, method)?;
    let method = hint.canonical_method?;
    match (hint.contract, method) {
        ("ContractManagement", "Destroy" | "Update") | ("OracleContract", "Request") => Some(false),
        _ if lookup(hash_le, method, call_flags).is_some() => Some(true),
        _ => None,
    }
}

fn return_type(value_type: ValueType, csharp_type: &'static str) -> NativeMethodReturnType {
    NativeMethodReturnType {
        value_type,
        csharp_type,
    }
}

fn parse_hash(hash_le: &str) -> Option<[u8; 20]> {
    if hash_le.len() != 40 {
        return None;
    }
    let mut hash = [0u8; 20];
    for (index, pair) in hash_le.as_bytes().chunks_exact(2).enumerate() {
        let pair = std::str::from_utf8(pair).ok()?;
        hash[index] = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STDLIB: &str = "C0EF39CEE0E4E925C6C2A06A79E1440DD86FCEAC";
    const CONTRACT_MANAGEMENT: &str = "FDA3FA4346EA532A258FC497DDADDB6437C9FDFF";
    const LEDGER: &str = "BEF2043140362A77C15099C7E64C12F700B665DA";
    const NEO: &str = "F563EA40BC283D4D0E05C48EA305B3F2A07340EF";
    const ROLE_MANAGEMENT: &str = "E295E391544C178AD94F03EC4DCDFF78534ECF49";
    const CRYPTO_LIB: &str = "1BF575AB1189688413610A35A12886CDE0B66C72";
    const NOTARY: &str = "3BEC3531119BBAD76DD044920B0DE6C3194FE1C1";
    const ORACLE: &str = "588717117E0AA81072AFAB71D2DD89FE7C4B92FE";
    const POLICY: &str = "7BC681C0A1F71D543457B68BBA8D5F9FDD4E5ECC";

    #[test]
    fn resolves_only_hash_bound_native_signatures() {
        let result = lookup(Some(STDLIB), "strLen", Some(0x0F)).expect("StdLib method");
        assert_eq!(result.value_type, ValueType::Integer);
        assert_eq!(result.csharp_type, "BigInteger");

        assert!(lookup(Some(STDLIB), "strLen", Some(0x01)).is_none());
        assert!(lookup(Some("00"), "strLen", Some(0x0F)).is_none());
        assert!(lookup(Some(STDLIB), "notAStdLibMethod", Some(0x0F)).is_none());
    }

    #[test]
    fn maps_framework_string_and_collection_returns() {
        let string = lookup(Some(STDLIB), "base58CheckEncode", Some(0x0F)).unwrap();
        assert_eq!(string.value_type, ValueType::ByteString);
        assert_eq!(string.csharp_type, "string");

        let array = lookup(Some(STDLIB), "stringSplit", Some(0x0F)).unwrap();
        assert_eq!(array.value_type, ValueType::Array);
        assert_eq!(array.csharp_type, "object[]");

        let bytes = lookup(Some(STDLIB), "base58CheckDecode", Some(0x0F)).unwrap();
        assert_eq!(bytes.value_type, ValueType::ByteString);
        assert_eq!(bytes.csharp_type, "ByteString");

        let json = lookup(Some(STDLIB), "jsonSerialize", Some(0x0F)).unwrap();
        assert_eq!(json.value_type, ValueType::ByteString);
        assert_eq!(json.csharp_type, "string");
    }

    #[test]
    fn maps_framework_native_contract_returns() {
        let current_index = lookup(Some(LEDGER), "currentIndex", Some(0x0F)).unwrap();
        assert_eq!(current_index.value_type, ValueType::Integer);
        assert_eq!(current_index.csharp_type, "uint");

        let signers = lookup(Some(LEDGER), "getTransactionSigners", Some(0x0F)).unwrap();
        assert_eq!(signers.value_type, ValueType::Array);
        assert_eq!(signers.csharp_type, "Signer[]");

        let balance = lookup(Some(NEO), "balanceOf", Some(0x0F)).unwrap();
        assert_eq!(balance.value_type, ValueType::Integer);
        assert_eq!(balance.csharp_type, "BigInteger");

        let designated = lookup(Some(ROLE_MANAGEMENT), "getDesignatedByRole", Some(0x0F)).unwrap();
        assert_eq!(designated.value_type, ValueType::Array);
        assert_eq!(designated.csharp_type, "ECPoint[]");

        let oracle = lookup(Some(ORACLE), "getPrice", Some(0x0F)).unwrap();
        assert_eq!(oracle.csharp_type, "long");

        let policy = lookup(Some(POLICY), "getExecFeeFactor", Some(0x0F)).unwrap();
        assert_eq!(policy.csharp_type, "uint");

        let notary = lookup(Some(NOTARY), "expirationOf", Some(0x0F)).unwrap();
        assert_eq!(notary.csharp_type, "uint");

        let crypto = lookup(Some(CRYPTO_LIB), "ripemd160", Some(0x0F)).unwrap();
        assert_eq!(crypto.csharp_type, "ByteString");
    }

    #[test]
    fn maps_additional_framework_native_returns() {
        let contract = lookup(Some(CONTRACT_MANAGEMENT), "getContract", Some(0x0F)).unwrap();
        assert_eq!(contract.value_type, ValueType::InteropInterface);
        assert_eq!(contract.csharp_type, "Contract");

        let hashes = lookup(Some(CONTRACT_MANAGEMENT), "getContractHashes", Some(0x0F)).unwrap();
        assert_eq!(hashes.value_type, ValueType::InteropInterface);
        assert_eq!(hashes.csharp_type, "Iterator<(int, UInt160)>");

        let bls = lookup(Some(CRYPTO_LIB), "bls12381Add", Some(0x0F)).unwrap();
        assert_eq!(bls.value_type, ValueType::Unknown);
        assert_eq!(bls.csharp_type, "object");

        let block = lookup(Some(LEDGER), "getBlock", Some(0x0F)).unwrap();
        assert_eq!(block.value_type, ValueType::InteropInterface);
        assert_eq!(block.csharp_type, "Block");

        let iterator = lookup(Some(POLICY), "getBlockedAccounts", Some(0x0F)).unwrap();
        assert_eq!(iterator.value_type, ValueType::InteropInterface);
        assert_eq!(iterator.csharp_type, "Iterator");

        let candidates = lookup(Some(NEO), "getCandidates", Some(0x0F)).unwrap();
        assert_eq!(candidates.value_type, ValueType::Array);
        assert_eq!(candidates.csharp_type, "(ECPoint, BigInteger)[]");

        let candidate_iterator = lookup(Some(NEO), "getAllCandidates", Some(0x0F)).unwrap();
        assert_eq!(candidate_iterator.value_type, ValueType::InteropInterface);
        assert_eq!(
            candidate_iterator.csharp_type,
            "Iterator<(ECPoint, BigInteger)>"
        );

        let account_state = lookup(Some(NEO), "getAccountState", Some(0x0F)).unwrap();
        assert_eq!(account_state.value_type, ValueType::InteropInterface);
        assert_eq!(account_state.csharp_type, "NeoAccountState");

        let deserialized = lookup(Some(STDLIB), "deserialize", Some(0x0F)).unwrap();
        assert_eq!(deserialized.value_type, ValueType::Unknown);
        assert_eq!(deserialized.csharp_type, "object");
    }

    #[test]
    fn distinguishes_known_native_void_methods_from_values() {
        let management = "FDA3FA4346EA532A258FC497DDADDB6437C9FDFF";
        assert_eq!(
            returns_value(Some(management), "destroy", Some(0x0F)),
            Some(false)
        );
        assert_eq!(
            returns_value(Some(management), "hasMethod", Some(0x0F)),
            Some(true)
        );
        assert_eq!(returns_value(Some(management), "destroy", Some(0x01)), None);
    }
}

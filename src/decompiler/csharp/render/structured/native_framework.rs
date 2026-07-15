//! C# framework bindings for catalogued native-contract method tokens.

/// Return the framework method spelling for a catalogued native method.
///
/// The native catalog describes VM-facing names, which are not always the
/// same as the C# framework spelling. It also contains protocol methods that
/// are not exposed by every framework assembly. Returning `None` keeps those
/// calls in the hash-preserving `Contract.Call` compatibility form.
pub(super) fn method_name<'a>(contract: &str, method: &'a str) -> Option<&'a str> {
    let mapped = match (contract, method) {
        ("CryptoLib", "recoverSecp256K1") => Some("RecoverSecp256K1"),
        ("CryptoLib", "ripemd160") => Some("Ripemd160"),
        ("CryptoLib", "verifyWithECDsa") => Some("VerifyWithECDsa"),
        ("LedgerContract", "getTransaction") => Some("GetTransaction"),
        ("NeoToken", "UnregisterCandidate") => Some("UnRegisterCandidate"),
        ("PolicyContract", "getAttributeFee") => Some("GetAttributeFee"),
        _ => None,
    };
    if mapped.is_some() {
        return mapped;
    }
    is_supported_contract(contract)
        .then_some(method)
        .filter(|method| is_supported_method(contract, method))
}

fn is_supported_contract(contract: &str) -> bool {
    matches!(
        contract,
        "ContractManagement"
            | "CryptoLib"
            | "LedgerContract"
            | "Notary"
            | "OracleContract"
            | "PolicyContract"
            | "RoleManagement"
            | "StdLib"
            | "Treasury"
            | "GasToken"
            | "NeoToken"
    )
}

fn is_supported_method(contract: &str, method: &str) -> bool {
    match contract {
        "ContractManagement" => matches!(
            method,
            "Deploy"
                | "Destroy"
                | "GetContract"
                | "GetContractById"
                | "GetContractHashes"
                | "GetMinimumDeploymentFee"
                | "HasMethod"
                | "IsContract"
                | "Update"
        ),
        "CryptoLib" => matches!(
            method,
            "Bls12381Add"
                | "Bls12381Deserialize"
                | "Bls12381Equal"
                | "Bls12381Mul"
                | "Bls12381Pairing"
                | "Bls12381Serialize"
                | "Keccak256"
                | "Murmur32"
                | "Sha256"
                | "VerifyWithECDsa"
                | "VerifyWithEd25519"
        ),
        "LedgerContract" => matches!(
            method,
            "CurrentHash"
                | "CurrentIndex"
                | "GetBlock"
                | "GetTransactionFromBlock"
                | "GetTransactionHeight"
                | "GetTransactionSigners"
                | "GetTransactionVMState"
        ),
        "Notary" => matches!(
            method,
            "BalanceOf"
                | "ExpirationOf"
                | "GetMaxNotValidBeforeDelta"
                | "LockDepositUntil"
                | "Verify"
                | "Withdraw"
        ),
        "OracleContract" => matches!(method, "GetPrice" | "Request"),
        "PolicyContract" => matches!(
            method,
            "GetAttributeFee"
                | "GetBlockedAccounts"
                | "GetExecFeeFactor"
                | "GetExecPicoFeeFactor"
                | "GetFeePerByte"
                | "GetStoragePrice"
                | "GetWhitelistFeeContracts"
                | "IsBlocked"
        ),
        "RoleManagement" => matches!(method, "GetDesignatedByRole"),
        "StdLib" => matches!(
            method,
            "Atoi"
                | "Base58CheckDecode"
                | "Base58CheckEncode"
                | "Base58Decode"
                | "Base58Encode"
                | "Base64Decode"
                | "Base64Encode"
                | "Base64UrlDecode"
                | "Base64UrlEncode"
                | "Deserialize"
                | "HexDecode"
                | "HexEncode"
                | "Itoa"
                | "JsonDeserialize"
                | "JsonSerialize"
                | "MemoryCompare"
                | "MemorySearch"
                | "Serialize"
                | "StrLen"
                | "StringSplit"
        ),
        "Treasury" => matches!(method, "Verify"),
        "GasToken" => matches!(
            method,
            "BalanceOf" | "Decimals" | "Symbol" | "TotalSupply" | "Transfer"
        ),
        "NeoToken" => matches!(
            method,
            "BalanceOf"
                | "Decimals"
                | "GetAccountState"
                | "GetAllCandidates"
                | "GetCandidateVote"
                | "GetCandidates"
                | "GetCommittee"
                | "GetCommitteeAddress"
                | "GetGasPerBlock"
                | "GetNextBlockValidators"
                | "GetRegisterPrice"
                | "RegisterCandidate"
                | "SetGasPerBlock"
                | "SetRegisterPrice"
                | "Symbol"
                | "TotalSupply"
                | "Transfer"
                | "UnclaimedGas"
                | "Vote"
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::method_name;

    #[test]
    fn maps_vm_method_casing_to_framework_spelling() {
        assert_eq!(
            method_name("CryptoLib", "recoverSecp256K1"),
            Some("RecoverSecp256K1")
        );
        assert_eq!(
            method_name("LedgerContract", "getTransaction"),
            Some("GetTransaction")
        );
        assert_eq!(
            method_name("NeoToken", "UnregisterCandidate"),
            Some("UnRegisterCandidate")
        );
    }

    #[test]
    fn rejects_catalog_methods_without_framework_bindings() {
        assert_eq!(method_name("Governance", "GetCommittee"), None);
        assert_eq!(method_name("OracleContract", "Finish"), None);
        assert_eq!(method_name("PolicyContract", "SetFeePerByte"), None);
    }
}

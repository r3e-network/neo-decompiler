//! Lookup information for Neo native contracts.

mod generated {
    include!("native_contracts_generated.rs");
}

/// Metadata describing a native contract.
pub use generated::NativeContractInfo;

/// Return the native contract that matches the provided script hash (little-endian bytes).
pub fn lookup(hash: &[u8; 20]) -> Option<&'static NativeContractInfo> {
    generated::NATIVE_CONTRACTS
        .iter()
        .find(|info| info.script_hash == *hash)
}

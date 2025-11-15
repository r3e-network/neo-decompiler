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

/// Return the full list of bundled native contracts.
pub fn all() -> &'static [NativeContractInfo] {
    generated::NATIVE_CONTRACTS
}

/// Additional metadata explaining how a method token maps to a native contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMethodHint {
    pub contract: &'static str,
    pub canonical_method: Option<&'static str>,
}

impl NativeMethodHint {
    pub fn formatted_label(&self, provided: &str) -> String {
        match self.canonical_method {
            Some(method) => format!("{}::{method}", self.contract),
            None => format!("{}::<unknown {provided}>", self.contract),
        }
    }

    pub fn has_exact_method(&self) -> bool {
        self.canonical_method.is_some()
    }
}

/// Return native contract guidance for the supplied method token.
pub fn describe_method_token(hash: &[u8; 20], method: &str) -> Option<NativeMethodHint> {
    let contract = lookup(hash)?;
    let canonical_method = contract
        .methods
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(method))
        .copied();
    Some(NativeMethodHint {
        contract: contract.name,
        canonical_method,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_known_native_method() {
        let info = &generated::NATIVE_CONTRACTS[0];
        let method = info.methods[0];
        let hint = describe_method_token(&info.script_hash, method).expect("hint");
        assert_eq!(hint.contract, info.name);
        assert_eq!(hint.canonical_method, Some(method));
    }

    #[test]
    fn falls_back_to_contract_name_when_method_unknown() {
        let info = &generated::NATIVE_CONTRACTS[0];
        let hint = describe_method_token(&info.script_hash, "NotAMethod").expect("hint");
        assert_eq!(hint.contract, info.name);
        assert!(hint.canonical_method.is_none());
    }
}

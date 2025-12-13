//! Lookup information for Neo native contracts.

#[allow(missing_docs)]
mod generated {
    include!("native_contracts_generated.rs");
}

/// Metadata describing a native contract.
pub use generated::NativeContractInfo;

const fn script_hash_lt(left: &[u8; 20], right: &[u8; 20]) -> bool {
    let mut i = 0usize;
    while i < 20 {
        if left[i] < right[i] {
            return true;
        }
        if left[i] > right[i] {
            return false;
        }
        i += 1;
    }
    false
}

const fn assert_native_contracts_sorted_by_hash(contracts: &[NativeContractInfo]) {
    let mut i = 1usize;
    while i < contracts.len() {
        if !script_hash_lt(&contracts[i - 1].script_hash, &contracts[i].script_hash) {
            panic!(
                "generated::NATIVE_CONTRACTS must be sorted by script_hash (strictly increasing)"
            );
        }
        i += 1;
    }
}

const _: () = assert_native_contracts_sorted_by_hash(generated::NATIVE_CONTRACTS);

/// Return the native contract that matches the provided script hash (little-endian bytes).
pub fn lookup(hash: &[u8; 20]) -> Option<&'static NativeContractInfo> {
    generated::NATIVE_CONTRACTS
        .binary_search_by_key(hash, |info| info.script_hash)
        .ok()
        .map(|index| &generated::NATIVE_CONTRACTS[index])
}

/// Return the full list of bundled native contracts.
pub fn all() -> &'static [NativeContractInfo] {
    generated::NATIVE_CONTRACTS
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Additional metadata explaining how a method token maps to a native contract.
///
/// This is returned by [`describe_method_token`] to help callers surface a friendly
/// label for native calls.
///
/// The [`NativeMethodHint::contract`] field always contains the canonical native
/// contract name. The [`NativeMethodHint::canonical_method`] field is `Some` when
/// the provided method name matches one of the known native methods.
pub struct NativeMethodHint {
    /// Canonical native contract name.
    pub contract: &'static str,
    /// Canonical method name when it could be resolved.
    pub canonical_method: Option<&'static str>,
}

impl NativeMethodHint {
    /// Format a label for displaying this hint.
    ///
    /// When [`NativeMethodHint::canonical_method`] is known, this returns
    /// `<contract>::<method>`. Otherwise it embeds the provided method name as
    /// `<contract>::<unknown <provided>>`.
    ///
    /// # Examples
    /// ```
    /// use neo_decompiler::native_contracts::NativeMethodHint;
    ///
    /// let hint = NativeMethodHint { contract: "Contract", canonical_method: Some("Method") };
    /// assert_eq!(hint.formatted_label("ignored"), "Contract::Method");
    ///
    /// let hint = NativeMethodHint { contract: "Contract", canonical_method: None };
    /// assert_eq!(hint.formatted_label("Provided"), "Contract::<unknown Provided>");
    /// ```
    pub fn formatted_label(&self, provided: &str) -> String {
        match self.canonical_method {
            Some(method) => format!("{}::{method}", self.contract),
            None => format!("{}::<unknown {provided}>", self.contract),
        }
    }

    /// Return `true` if the hint resolved the provided method to a known native method.
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
mod tests;

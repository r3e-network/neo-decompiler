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

#[test]
fn describe_method_token_prefers_exact_case_match() {
    let info = generated::NATIVE_CONTRACTS
        .iter()
        .find(|info| info.name == "CryptoLib")
        .expect("CryptoLib contract");
    let hint = describe_method_token(&info.script_hash, "verifyWithECDsa").expect("hint");
    assert_eq!(hint.canonical_method, Some("verifyWithECDsa"));
}

#[test]
fn lookup_finds_every_native_contract() {
    for info in all() {
        let got = lookup(&info.script_hash).expect("expected contract to be present");
        assert_eq!(got.name, info.name);
        assert_eq!(got.script_hash, info.script_hash);
        assert_eq!(got.methods, info.methods);
    }
}

#[test]
fn lookup_unknown_hash_returns_none() {
    assert!(lookup(&[0u8; 20]).is_none());
    assert!(lookup(&[0xFFu8; 20]).is_none());
}

#[test]
fn native_contract_table_is_sorted_by_hash() {
    let contracts = all();
    for window in contracts.windows(2) {
        assert!(window[0].script_hash < window[1].script_hash);
    }
}

#[test]
fn native_method_hint_helpers_report_expected_state() {
    let with_method = NativeMethodHint {
        contract: "Contract",
        canonical_method: Some("SomeMethod"),
    };
    assert!(with_method.has_exact_method());
    assert_eq!(
        with_method.formatted_label("Provided"),
        "Contract::SomeMethod"
    );

    let without_method = NativeMethodHint {
        contract: "Contract",
        canonical_method: None,
    };
    assert!(!without_method.has_exact_method());
    assert_eq!(
        without_method.formatted_label("Provided"),
        "Contract::<unknown Provided>"
    );
}

#[test]
fn native_contract_catalog_includes_latest_core_contracts() {
    let names: Vec<&str> = all().iter().map(|info| info.name).collect();
    assert!(
        names.contains(&"TokenManagement"),
        "native contract table must include TokenManagement"
    );
    assert!(
        names.contains(&"Governance"),
        "native contract table must include Governance"
    );
}

#[test]
fn native_contract_catalog_keeps_legacy_token_contracts() {
    let names: Vec<&str> = all().iter().map(|info| info.name).collect();
    assert!(
        names.contains(&"NeoToken"),
        "native contract table must include NeoToken for compatibility"
    );
    assert!(
        names.contains(&"GasToken"),
        "native contract table must include GasToken for compatibility"
    );
}

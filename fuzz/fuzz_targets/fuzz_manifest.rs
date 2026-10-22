#![no_main]

use libfuzzer_sys::fuzz_target;
use neo_decompiler::ContractManifest;

fuzz_target!(|data: &[u8]| {
    let Ok(manifest) = ContractManifest::from_bytes(data) else {
        // A manifest the parser rejects has nothing to check.
        return;
    };

    // The method and event counts are what a consumer matches against
    // the contract's own methods, so they have to survive the trip
    // through the strict parser unchanged.
    //
    // The strict parser is a second, independent reading of the same
    // document. A manifest that the tolerant parser accepts but the
    // strict one reads differently would mean the two disagree about
    // what the contract exposes -- and the strict one is the one that
    // guards the chain.
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(strict) = ContractManifest::from_json_str_strict(text) {
        assert_eq!(
            strict.abi.methods.len(),
            manifest.abi.methods.len(),
            "the tolerant and the strict parser disagree about the method count"
        );
        assert_eq!(
            strict.abi.events.len(),
            manifest.abi.events.len(),
            "the tolerant and the strict parser disagree about the event count"
        );
        assert_eq!(
            strict.name, manifest.name,
            "the tolerant and the strict parser disagree about the contract name"
        );
    }

    // The same document read through the JSON string entry point has to
    // produce the same manifest. The two entry points take different
    // paths -- one decodes UTF-8 itself, the other takes a string -- and
    // a difference between them would show up as a manifest that behaves
    // differently depending on how it was loaded.
    let reparsed = ContractManifest::from_json_str(text).expect("the manifest already parsed");
    assert_eq!(
        reparsed.abi.methods.len(),
        manifest.abi.methods.len(),
        "the method count changed on the round trip"
    );
    assert_eq!(
        reparsed.abi.events.len(),
        manifest.abi.events.len(),
        "the event count changed on the round trip"
    );
    assert_eq!(
        reparsed.name, manifest.name,
        "the contract name changed on the round trip"
    );
});

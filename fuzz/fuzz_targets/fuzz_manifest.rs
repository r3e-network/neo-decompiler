#![no_main]

use libfuzzer_sys::fuzz_target;
use neo_decompiler::ContractManifest;

fuzz_target!(|data: &[u8]| {
    // We don't care about the result, only that it doesn't panic
    let _ = ContractManifest::from_bytes(data);
});

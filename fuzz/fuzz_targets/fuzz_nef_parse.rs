#![no_main]

use libfuzzer_sys::fuzz_target;
use neo_decompiler::NefParser;

fuzz_target!(|data: &[u8]| {
    let parser = NefParser::new();
    let Ok(nef) = parser.parse(data) else {
        // A container the parser rejects has nothing to check. The
        // rejection itself is the interesting part, and the parser is
        // exercised by returning here.
        return;
    };

    // The payload length has to match the bytes that were actually
    // written. A parser that computes the length differently from the
    // encoder would accept a container whose checksum covers the wrong
    // span, and the mismatch would only show up as a hash that does not
    // match on the far side.
    let expected_len = nef.payload_len() + 4; // payload plus the checksum
    assert!(
        expected_len <= data.len(),
        "the parsed payload claims {expected_len} bytes, but the container is {} bytes",
        data.len()
    );

    // The re-serialized container has to parse back to the same content.
    // A container that changes when it is read and written back cannot
    // be checked, diffed, or reproduced by anything that consumes it.
    let encoded = nef.to_bytes();
    let reparsed = NefParser::new()
        .parse(&encoded)
        .expect("a container the crate produced must parse back");
    assert_eq!(reparsed.script, nef.script, "the script changed");
    assert_eq!(
        reparsed.method_tokens, nef.method_tokens,
        "the method tokens changed"
    );
    assert_eq!(
        reparsed.header.source, nef.header.source,
        "the source changed"
    );
    assert_eq!(
        reparsed.header.compiler, nef.header.compiler,
        "the compiler field changed"
    );

    // The checksum covers the payload and nothing else, so the serialized
    // form has to be exactly the payload plus the checksum.
    assert_eq!(
        encoded.len(),
        nef.payload_len() + 4,
        "the serialized container does not match the declared payload length"
    );
});

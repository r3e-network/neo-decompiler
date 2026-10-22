use super::*;

/// Build a container with a method token, so the token table is exercised.
fn build_sample_with_token(script: &[u8], method: &str) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    let mut compiler = [0u8; 64];
    let name = b"neo-sample";
    compiler[..name.len()].copy_from_slice(name);
    data.extend_from_slice(&compiler);
    // source
    data.push(0);
    // reserved byte
    data.push(0);
    // one method token
    data.push(1);
    data.extend_from_slice(&[0xAB; 20]);
    write_varint(&mut data, method.len() as u32);
    data.extend_from_slice(method.as_bytes());
    data.extend_from_slice(&2u16.to_le_bytes());
    data.push(1); // has return value
    data.push(0x04); // call flags: allow call
                     // reserved word
    data.extend_from_slice(&0u16.to_le_bytes());
    // script
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());
    data
}

#[test]
fn round_trips_a_minimal_container() {
    let bytes = build_sample(&[0x10, 0x11, 0x40]);
    let nef = NefParser::new().parse(&bytes).expect("parse succeeds");

    let encoded = nef.to_bytes();
    assert_eq!(
        encoded, bytes,
        "a canonical container round trips byte for byte"
    );
}

#[test]
fn round_trips_a_container_with_a_method_token() {
    let bytes = build_sample_with_token(&[0x40, 0x41], "transfer");
    let nef = NefParser::new().parse(&bytes).expect("parse succeeds");
    assert_eq!(nef.method_tokens.len(), 1);

    let encoded = nef.to_bytes();
    assert_eq!(
        encoded, bytes,
        "a canonical container round trips byte for byte"
    );
}

#[test]
fn round_trips_a_container_with_a_source_string() {
    let script = vec![0x40];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    write_varint(&mut data, 5);
    data.extend_from_slice(b"src:5");
    data.push(0); // reserved byte
    data.push(0); // zero tokens
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let nef = NefParser::new().parse(&data).expect("parse succeeds");
    assert_eq!(nef.header.source, "src:5");
    assert_eq!(nef.to_bytes(), data);
}

#[test]
fn serialized_container_is_accepted_by_the_parser() {
    // The point of the serializer is that the result can be read back. This
    // checks the case where the script is large enough to need a multi-byte
    // varint length prefix, which is where a hand-written serializer is most
    // likely to get the prefix width wrong.
    let script: Vec<u8> = (0..600u32).map(|i| (i % 256) as u8).collect();
    let bytes = build_sample(&script);
    let nef = NefParser::new().parse(&bytes).expect("parse succeeds");

    let encoded = nef.to_bytes();
    let reparsed = NefParser::new()
        .parse(&encoded)
        .expect("the serialized container parses");
    assert_eq!(reparsed.script, script);
    assert_eq!(reparsed.header.magic, nef.header.magic);
    assert_eq!(reparsed.header.compiler, nef.header.compiler);
    assert_eq!(reparsed.method_tokens, nef.method_tokens);
}

#[test]
fn serialized_length_matches_the_payload_length() {
    // `payload_len` is the size of everything before the checksum. The
    // serializer must agree with it, otherwise the two descriptions of the
    // container drift apart.
    let script: Vec<u8> = (0..300u32).map(|i| (i % 256) as u8).collect();
    let bytes = build_sample(&script);
    let nef = NefParser::new().parse(&bytes).expect("parse succeeds");

    let encoded = nef.to_bytes();
    assert_eq!(encoded.len(), nef.payload_len() + CHECKSUM_SIZE);
}

#[test]
fn serializing_a_mutated_script_changes_the_checksum() {
    // The fuzz target rebuilds containers from a mutated script. If the
    // checksum were not recomputed, the parser would reject every rebuilt
    // container and the mutation would never reach the decompiler.
    let bytes = build_sample(&[0x10, 0x11, 0x40]);
    let mut nef = NefParser::new().parse(&bytes).expect("parse succeeds");
    let original_checksum = nef.checksum;

    nef.script = vec![0x20, 0x21, 0x40];
    let encoded = nef.to_bytes();

    let reparsed = NefParser::new()
        .parse(&encoded)
        .expect("a rebuilt container parses");
    assert_eq!(reparsed.script, vec![0x20, 0x21, 0x40]);
    assert_ne!(reparsed.checksum, original_checksum);
}

#[test]
fn normalizes_a_non_canonical_varint() {
    // The parser accepts a non-canonical length prefix, and the reference
    // implementation accepts it too. The serializer writes the canonical
    // form, so the bytes change but the meaning does not.
    let script = vec![0x40];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    data.push(0xFD);
    data.extend_from_slice(&5u16.to_le_bytes());
    data.extend_from_slice(b"src:5");
    data.push(0);
    data.push(0);
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let nef = NefParser::new().parse(&data).expect("parse succeeds");
    let encoded = nef.to_bytes();
    assert_ne!(encoded, data, "the non-canonical prefix is normalized");

    let reparsed = NefParser::new().parse(&encoded).expect("parses");
    assert_eq!(reparsed.header.source, "src:5");
    assert_eq!(reparsed.script, script);
}

#[test]
fn builds_a_container_from_its_parts() {
    // The struct is `#[non_exhaustive]`, so a caller outside the crate
    // cannot write the literal. The constructor is the way to build one,
    // and the container it produces has to be accepted by the parser.
    let file = NefFile::new(vec![0x10, 0x11, 0x40])
        .with_compiler("neo-test")
        .with_source("src:test");

    let bytes = file.to_bytes();
    let parsed = NefParser::new().parse(&bytes).expect("parse succeeds");

    assert_eq!(parsed.script, vec![0x10, 0x11, 0x40]);
    assert_eq!(parsed.header.compiler, "neo-test");
    assert_eq!(parsed.header.source, "src:test");
    assert_eq!(parsed.header.magic, *b"NEF3");
}

#[test]
fn rejects_a_container_without_a_script() {
    // An empty script is not a legal container. The parser rejects it,
    // so the constructor still produces a container the parser refuses
    // rather than one that is silently accepted.
    let file = NefFile::new(Vec::new());
    let bytes = file.to_bytes();
    assert!(
        NefParser::new().parse(&bytes).is_err(),
        "a container without a script must not be accepted"
    );
}

#[test]
fn truncates_a_compiler_field_that_is_too_long() {
    // The compiler field is a fixed 64-byte slot, so a longer value is
    // truncated rather than changing the layout of the container.
    let long = "c".repeat(200);
    let file = NefFile::new(vec![0x40]).with_compiler(long);
    let bytes = file.to_bytes();
    let parsed = NefParser::new().parse(&bytes).expect("parse succeeds");
    assert_eq!(parsed.header.compiler, "c".repeat(64));
}

#[test]
fn refuses_a_container_the_parser_would_reject() {
    // The parser enforces limits on the script length, the source length,
    // and the total file size. A container that breaks one of them cannot
    // be read back, so writing it produces bytes that look like a
    // container and are not one.
    let oversized_script = NefFile::new(vec![0x40; super::super::MAX_SCRIPT_LEN + 1]);
    assert!(
        oversized_script.try_to_bytes().is_err(),
        "a container with an oversized script must be refused"
    );

    let oversized_source =
        NefFile::new(vec![0x40]).with_source("s".repeat(super::super::MAX_SOURCE_LEN + 1));
    assert!(
        oversized_source.try_to_bytes().is_err(),
        "a container with an oversized source must be refused"
    );
}

#[test]
fn accepts_a_container_at_the_limit() {
    // The limit itself is allowed. Only what is past it is refused.
    let at_limit = NefFile::new(vec![0x40; super::super::MAX_SCRIPT_LEN]);
    let bytes = at_limit
        .try_to_bytes()
        .expect("a container at the limit must be accepted");
    assert!(bytes.len() as u64 <= super::super::MAX_NEF_FILE_SIZE);
}

#[test]
fn the_file_limit_cannot_be_reached_by_a_valid_container() {
    // The parser enforces three limits: the script length, the source
    // length, and the total file size. The file limit is the one that
    // cannot be reached, because the other two are lower.
    //
    // The largest script the parser accepts is 512 KiB, the largest
    // method token table is 128 entries, and the source is at most 256
    // bytes. Together they come to roughly 517 KiB, well under the 1 MiB
    // file limit. So a container that breaks the file limit has already
    // broken one of the others.
    //
    // The check is kept because the limits are independent constants and
    // one of them could be raised later. This test records why it is
    // unreachable today, so a change that makes it reachable is visible.
    let mut file = NefFile::new(vec![0x40; super::super::MAX_SCRIPT_LEN]);
    file.method_tokens = (0..super::super::MAX_METHOD_TOKENS)
        .map(|index| MethodToken {
            hash: [0xAB; 20],
            method: format!("m{index}"),
            parameters_count: 1,
            has_return_value: false,
            call_flags: 0x04,
        })
        .collect();

    let bytes = file
        .try_to_bytes()
        .expect("a container at every other limit must be accepted");
    let size = bytes.len() as u64;
    assert!(
        size < super::super::MAX_NEF_FILE_SIZE,
        "a container at the other limits reached {size} bytes, which is past the {}-byte file limit",
        super::super::MAX_NEF_FILE_SIZE
    );
}

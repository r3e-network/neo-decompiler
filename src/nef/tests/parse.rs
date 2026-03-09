use super::*;

#[test]
fn parses_valid_nef() {
    let script = vec![0x10, 0x11, 0x40];
    let bytes = build_sample(&script);
    let nef = NefParser::new().parse(&bytes).expect("parse succeeds");

    assert_eq!(nef.header.magic, MAGIC);
    assert_eq!(nef.header.compiler, "neo-sample");
    assert!(nef.header.source.is_empty());
    assert_eq!(nef.script, script);
    assert!(nef.method_tokens.is_empty());
    assert_eq!(
        util::format_hash(&nef.script_hash()),
        util::format_hash(&util::hash160(&[0x10, 0x11, 0x40]))
    );
}

#[test]
fn rejects_bad_magic() {
    let mut bytes = build_sample(&[0x40]);
    bytes[0] = b'X';
    let err = NefParser::new().parse(&bytes).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::InvalidMagic { .. })
    ));
}

#[test]
fn rejects_bad_checksum() {
    let mut bytes = build_sample(&[0x40]);
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    let err = NefParser::new().parse(&bytes).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::ChecksumMismatch { .. })
    ));
}

#[test]
fn rejects_truncated_checksum_instead_of_panicking() {
    let mut bytes = build_sample(&[0x40]);
    bytes.pop(); // drop part of the checksum trailer
    let err = NefParser::new().parse(&bytes).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::UnexpectedEof { .. })
    ));
}

#[test]
fn rejects_trailing_bytes() {
    let script = vec![0x40];
    let bytes = build_sample(&script);
    let mut with_extra = bytes;
    with_extra.push(0x99);

    let err = NefParser::new().parse(&with_extra).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::TrailingData { extra: 1 })
    ));
}

#[test]
fn rejects_nonzero_reserved_byte() {
    let mut bytes = build_sample(&[0x40]);
    bytes[69] = 0x01;

    let err = NefParser::new().parse(&bytes).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::ReservedByteNonZero {
            offset: 69,
            value: 0x01
        })
    ));
}

#[test]
fn rejects_nonzero_reserved_word() {
    let mut bytes = build_sample(&[0x40]);
    let reserved_word_offset = 4 + 64 + 1 + 1 + 1;
    bytes[reserved_word_offset] = 0x34;
    bytes[reserved_word_offset + 1] = 0x12;
    let checksum = NefParser::calculate_checksum(&bytes[..bytes.len() - 4]);
    let checksum_offset = bytes.len() - 4;
    bytes[checksum_offset..].copy_from_slice(&checksum.to_le_bytes());

    let err = NefParser::new().parse(&bytes).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::ReservedWordNonZero { offset, value: 0x1234 })
            if offset == reserved_word_offset
    ));
}

#[test]
fn rejects_oversized_u64_varint_for_source_length() {
    let script = vec![0x40];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    data.push(0xFF);
    data.extend_from_slice(&(u32::MAX as u64 + 1).to_le_bytes());
    data.push(0);
    data.push(0);
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let err = NefParser::new().parse(&data).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::IntegerOverflow { offset: 68 })
    ));
}

#[test]
fn rejects_non_canonical_varint_for_source_length() {
    let script = vec![0x40];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    data.push(0xFD);
    data.extend_from_slice(&0u16.to_le_bytes());
    data.push(0);
    data.push(0);
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let err = NefParser::new().parse(&data).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::NonCanonicalVarInt { offset: 68 })
    ));
}

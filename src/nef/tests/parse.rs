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
    let mut with_extra = bytes.clone();
    with_extra.push(0x99);

    let err = NefParser::new().parse(&with_extra).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::TrailingData { extra: 1 })
    ));
}

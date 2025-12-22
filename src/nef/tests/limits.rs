use super::*;

#[test]
fn rejects_source_too_long() {
    let script = vec![0x40];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    let long_source = "a".repeat(MAX_SOURCE_LEN + 1);
    write_varint(&mut data, long_source.len() as u32);
    data.extend_from_slice(long_source.as_bytes());
    data.push(0); // reserved byte
    data.push(0); // zero tokens
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let err = NefParser::new().parse(&data).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::SourceTooLong { .. })
    ));
}

#[test]
fn rejects_source_length_before_allocation() {
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    write_varint(&mut data, (MAX_SOURCE_LEN + 1) as u32);
    data.push(0); // ensure length clears the initial size guard

    let err = NefParser::new().parse(&data).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::SourceTooLong { .. })
    ));
}

#[test]
fn rejects_script_too_large() {
    let script = vec![0u8; MAX_SCRIPT_LEN + 1];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    data.push(0); // source
    data.push(0); // reserved
    data.push(0); // zero tokens
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let err = NefParser::new().parse(&data).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::ScriptTooLarge { .. })
    ));
}

#[test]
fn rejects_script_length_before_allocation() {
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    data.push(0); // source length zero
    data.push(0); // reserved byte
    data.push(0); // zero tokens
    data.extend_from_slice(&0u16.to_le_bytes()); // reserved word
    write_varint(&mut data, (MAX_SCRIPT_LEN + 1) as u32);

    let err = NefParser::new().parse(&data).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::ScriptTooLarge { .. })
    ));
}

#[test]
fn rejects_files_larger_than_limit() {
    let data = vec![0u8; MAX_NEF_FILE_SIZE as usize + 1];
    let err = NefParser::new().parse(&data).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::FileTooLarge { .. })
    ));
}

use super::super::*;

#[test]
fn rejects_overlong_method_token_name() {
    let script = vec![0x40];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    data.push(0); // source (empty)
    data.push(0); // reserved byte

    data.push(1); // one method token
    data.extend_from_slice(&[0x22; 20]); // hash
                                         // declare a method name longer than the cap without writing the payload
    write_varint(&mut data, (MAX_METHOD_NAME_LEN + 1) as u32);
    data.push(0); // params lo
    data.push(0); // params hi
    data.push(0); // return flag
    data.push(0); // call flags

    data.extend_from_slice(&0u16.to_le_bytes()); // reserved word
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let err = NefParser::new().parse(&data).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::InvalidMethodToken { .. })
    ));
}

#[test]
fn rejects_method_name_with_leading_underscore() {
    let script = vec![0x40];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    data.push(0); // source
    data.push(0); // reserved
    data.push(1); // one token
    data.extend_from_slice(&[0x22; 20]);
    write_varint(&mut data, 2);
    data.extend_from_slice(b"_x");
    data.extend_from_slice(&0u16.to_le_bytes());
    data.push(0); // no return
    data.push(0x10); // call flags (AllowNotify)
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let err = NefParser::new().parse(&data).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::MethodNameInvalid { .. })
    ));
}

#[test]
fn rejects_call_flags_with_unsupported_bits() {
    let script = vec![0x40];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    data.push(0); // source
    data.push(0); // reserved
    data.push(1); // one token
    data.extend_from_slice(&[0x33; 20]);
    write_varint(&mut data, 3);
    data.extend_from_slice(b"foo");
    data.extend_from_slice(&0u16.to_le_bytes());
    data.push(0); // no return
    data.push(0x80); // unsupported flag bit
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let err = NefParser::new().parse(&data).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::CallFlagsInvalid { .. })
    ));
}

#[test]
fn rejects_too_many_method_tokens() {
    let script = vec![0x40];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    data.push(0); // source
    data.push(0); // reserved
                  // declare more than allowed tokens
    write_varint(&mut data, (MAX_METHOD_TOKENS + 1) as u32);
    // no token payload needed because parser should error on count alone
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let err = NefParser::new().parse(&data).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::TooManyMethodTokens { .. })
    ));
}

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
    data.push(0x10); // unsupported call flag bit (name validation happens first)
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
    data.push(0x10); // first bit outside CallFlags::All
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let err = NefParser::new().parse(&data).unwrap_err();
    match err {
        crate::error::Error::Nef(NefError::CallFlagsInvalid { flags, allowed }) => {
            assert_eq!(flags, 0x10);
            assert_eq!(allowed, 0x0F);
        }
        other => panic!("expected CallFlagsInvalid, got {other:?}"),
    }
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

#[test]
fn rejects_oversized_u64_varint_for_method_token_count() {
    let script = vec![0x40];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    data.push(0); // source
    data.push(0); // reserved
    data.push(0xFF);
    data.extend_from_slice(&(u32::MAX as u64 + 1).to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let err = NefParser::new().parse(&data).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::IntegerOverflow { offset: 70 })
    ));
}

#[test]
fn accepts_non_canonical_varint_for_method_token_count() {
    // Matches the reference reader: `FD 00 00` is an overlong encoding of
    // zero, but C# MemoryReader.ReadVarInt accepts it, so we must too.
    let script = vec![0x40];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    data.push(0); // source
    data.push(0); // reserved
    data.push(0xFD);
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let nef = NefParser::new().parse(&data).expect("parse succeeds");
    assert!(nef.method_tokens.is_empty());
}

#[test]
fn rejects_method_name_longer_than_32_bytes() {
    // Reference: MethodToken.Deserialize uses ReadVarString(32).
    let script = vec![0x40];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    data.push(0); // source
    data.push(0); // reserved
    data.push(1); // one token
    data.extend_from_slice(&[0x22; 20]);
    let name = "a".repeat(33);
    write_varint(&mut data, name.len() as u32);
    data.extend_from_slice(name.as_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.push(0); // no return
    data.push(0x0F); // call flags
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let err = NefParser::new().parse(&data).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::InvalidMethodToken { index: 0 })
    ));
}

#[test]
fn rejects_more_than_128_method_tokens_and_accepts_exactly_128() {
    // Reference: NefFile.Deserialize reads ReadSerializableArray<MethodToken>(128).
    assert_eq!(MAX_METHOD_TOKENS, 128);

    let build = |count: u32| {
        let script = vec![0x40];
        let mut data = Vec::new();
        data.extend_from_slice(&MAGIC);
        data.extend_from_slice(&[0u8; 64]);
        data.push(0); // source
        data.push(0); // reserved
        write_varint(&mut data, count);
        for index in 0..count {
            data.extend_from_slice(&[index as u8; 20]);
            write_varint(&mut data, 3);
            data.extend_from_slice(b"foo");
            data.extend_from_slice(&0u16.to_le_bytes());
            data.push(0); // no return
            data.push(0x0F); // call flags
        }
        data.extend_from_slice(&0u16.to_le_bytes());
        write_varint(&mut data, script.len() as u32);
        data.extend_from_slice(&script);
        let checksum = NefParser::calculate_checksum(&data);
        data.extend_from_slice(&checksum.to_le_bytes());
        data
    };

    let nef = NefParser::new()
        .parse(&build(128))
        .expect("128 tokens parse");
    assert_eq!(nef.method_tokens.len(), 128);

    let err = NefParser::new().parse(&build(129)).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Nef(NefError::TooManyMethodTokens {
            count: 129,
            max: 128
        })
    ));
}

#[test]
fn accepts_method_name_of_exactly_32_bytes() {
    let script = vec![0x40];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    data.extend_from_slice(&[0u8; 64]);
    data.push(0); // source
    data.push(0); // reserved
    data.push(1); // one token
    data.extend_from_slice(&[0x22; 20]);
    let name = "a".repeat(32);
    write_varint(&mut data, name.len() as u32);
    data.extend_from_slice(name.as_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.push(0); // no return
    data.push(0x0F); // call flags
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let nef = NefParser::new().parse(&data).expect("parse succeeds");
    assert_eq!(nef.method_tokens[0].method, name);
}

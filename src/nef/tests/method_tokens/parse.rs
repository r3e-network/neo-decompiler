use super::super::*;

#[test]
fn parses_method_tokens() {
    let script = vec![0x40];
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    let mut compiler = [0u8; 64];
    compiler[..4].copy_from_slice(b"test");
    data.extend_from_slice(&compiler);
    // source (empty)
    data.push(0);
    // reserved byte
    data.push(0);

    // one method token
    data.push(1); // count
    data.extend_from_slice(&[0x11; 20]);
    write_varint(&mut data, 3);
    data.extend_from_slice(b"foo");
    // params
    data.extend_from_slice(&2u16.to_le_bytes());
    // return flag (true)
    data.push(1);
    // call flags (0x0F)
    data.push(0x0F);

    // reserved word
    data.extend_from_slice(&0u16.to_le_bytes());
    // script
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(&script);

    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());

    let nef = NefParser::new().parse(&data).expect("parse succeeds");
    assert_eq!(nef.method_tokens.len(), 1);
    let token = &nef.method_tokens[0];
    assert_eq!(token.method, "foo");
    assert_eq!(token.parameters_count, 2);
    assert!(token.has_return_value);
    assert_eq!(token.call_flags, 0x0F);
}

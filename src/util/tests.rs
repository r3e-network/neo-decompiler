use super::*;

#[test]
fn writes_upper_hex() {
    let bytes = [0xDE, 0xAD, 0xBE, 0xEF];
    assert_eq!(upper_hex_string(&bytes), "DEADBEEF");
}

#[test]
fn formats_hashes_in_both_endianness() {
    let bytes = [0x01, 0x23, 0x45, 0x67];
    assert_eq!(format_hash(&bytes), "01234567");
    assert_eq!(format_hash_be(&bytes), "67452301");
}

#[test]
fn computes_hash160_little_endian() {
    let script = [0x10, 0x11, 0x9E, 0x40];
    let hash = hash160(&script);
    assert_eq!(
        format_hash(&hash),
        "9DE87DC65A6A581E502CAE845C6F13645B10C5EA"
    );
    assert_eq!(
        format_hash_be(&hash),
        "EAC5105B64136F5C84AE2C501E586A5AC67DE89D"
    );
}

use super::*;
use crate::error::NefError;
use crate::util;

fn write_varint(buf: &mut Vec<u8>, value: u32) {
    match value {
        0x00..=0xFC => buf.push(value as u8),
        0xFD..=0xFFFF => {
            buf.push(0xFD);
            buf.extend_from_slice(&(value as u16).to_le_bytes());
        }
        _ => {
            buf.push(0xFE);
            buf.extend_from_slice(&value.to_le_bytes());
        }
    }
}

fn build_sample(payload_script: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&MAGIC);
    let mut compiler = [0u8; 64];
    let name = b"neo-sample";
    compiler[..name.len()].copy_from_slice(name);
    data.extend_from_slice(&compiler);
    // source (empty string)
    data.push(0);
    // reserved byte
    data.push(0);
    // method tokens: empty set
    data.push(0);
    // reserved word
    data.extend_from_slice(&0u16.to_le_bytes());
    // script
    write_varint(&mut data, payload_script.len() as u32);
    data.extend_from_slice(payload_script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());
    data
}

mod flags;
mod limits;
mod method_tokens;
mod parse;

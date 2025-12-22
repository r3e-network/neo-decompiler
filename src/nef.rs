//! Neo N3 NEF container parsing and helpers.

/// Maximum file size allowed for NEF files (10 MiB).
pub const MAX_NEF_FILE_SIZE: u64 = 10 * 1024 * 1024;
const FIXED_HEADER_SIZE: usize = 68;
const CHECKSUM_SIZE: usize = 4;
const MAGIC: [u8; 4] = *b"NEF3";
const MAX_SOURCE_LEN: usize = 256;
const MAX_METHOD_TOKENS: usize = 128;
const MAX_SCRIPT_LEN: usize = 1_048_576; // ExecutionEngineLimits.Default.MaxItemSize
const MAX_METHOD_NAME_LEN: usize = 1_024;
const CALL_FLAG_READ_STATES: u8 = 0x01;
const CALL_FLAG_WRITE_STATES: u8 = 0x02;
const CALL_FLAG_ALLOW_CALL: u8 = 0x04;
const CALL_FLAG_ALLOW_NOTIFY: u8 = 0x08;
const CALL_FLAGS_ALLOWED_MASK: u8 =
    CALL_FLAG_READ_STATES | CALL_FLAG_WRITE_STATES | CALL_FLAG_ALLOW_CALL | CALL_FLAG_ALLOW_NOTIFY;

mod encoding;
mod flags;
mod parser;
mod types;

pub use flags::{call_flag_labels, describe_call_flags};
pub use parser::NefParser;
pub use types::{MethodToken, NefFile, NefHeader};

#[cfg(test)]
mod tests;

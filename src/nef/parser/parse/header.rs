use super::{read_varstring, NefError, Result, MAGIC, MAX_SOURCE_LEN};

pub(super) fn read_magic(bytes: &[u8], offset: &mut usize) -> Result<[u8; 4]> {
    let magic_slice = bytes
        .get(*offset..*offset + 4)
        .ok_or(NefError::UnexpectedEof { offset: *offset })?;
    let mut magic = [0u8; 4];
    magic.copy_from_slice(magic_slice);
    *offset += 4;

    if magic != MAGIC {
        return Err(NefError::InvalidMagic {
            expected: MAGIC,
            actual: magic,
        }
        .into());
    }

    Ok(magic)
}

pub(super) fn read_compiler(bytes: &[u8], offset: &mut usize) -> Result<String> {
    let compiler_start = *offset;
    let compiler_end = compiler_start + 64;
    let compiler_bytes =
        bytes
            .get(compiler_start..compiler_end)
            .ok_or(NefError::UnexpectedEof {
                offset: compiler_start,
            })?;
    *offset = compiler_end;

    let compiler_len = compiler_bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(compiler_bytes.len());
    let compiler = std::str::from_utf8(&compiler_bytes[..compiler_len])
        .map_err(|_| NefError::InvalidCompiler)?
        .to_string();
    Ok(compiler)
}

pub(super) fn read_source(bytes: &[u8], offset: &mut usize) -> Result<String> {
    let (source, source_len) = read_varstring(bytes, *offset, MAX_SOURCE_LEN)?;
    *offset += source_len;
    Ok(source)
}

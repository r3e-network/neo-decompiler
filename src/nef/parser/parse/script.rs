use super::{read_varbytes, NefError, Result, MAX_SCRIPT_LEN};

pub(super) fn read_script(bytes: &[u8], offset: &mut usize) -> Result<Vec<u8>> {
    let (script, script_len) = read_varbytes(bytes, *offset, MAX_SCRIPT_LEN)?;
    if script.is_empty() {
        return Err(NefError::EmptyScript.into());
    }
    *offset += script_len;
    Ok(script)
}

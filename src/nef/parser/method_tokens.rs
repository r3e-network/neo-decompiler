use crate::error::{NefError, Result};

use super::super::encoding::read_varint;
use super::super::types::MethodToken;
use super::NefParser;

impl NefParser {
    pub(super) fn parse_method_tokens(
        &self,
        bytes: &[u8],
        mut offset: usize,
    ) -> Result<(Vec<MethodToken>, usize)> {
        let start = offset;
        let (count, varint_len) = read_varint(bytes, offset)?;
        offset += varint_len;

        if count as usize > super::super::MAX_METHOD_TOKENS {
            return Err(NefError::TooManyMethodTokens {
                count: count as usize,
                max: super::super::MAX_METHOD_TOKENS,
            }
            .into());
        }

        let mut tokens = Vec::with_capacity(count as usize);
        for index in 0..count as usize {
            let hash_start = offset;
            let hash_end = hash_start + 20;
            let hash_slice = bytes
                .get(hash_start..hash_end)
                .ok_or(NefError::UnexpectedEof { offset: hash_start })?;
            let mut hash = [0u8; 20];
            hash.copy_from_slice(hash_slice);
            offset = hash_end;

            let (method_len, method_varint) = read_varint(bytes, offset)?;
            offset += method_varint;
            if method_len as usize > super::super::MAX_METHOD_NAME_LEN {
                return Err(NefError::InvalidMethodToken { index }.into());
            }
            let method_end = offset + method_len as usize;
            let method_bytes = bytes
                .get(offset..method_end)
                .ok_or(NefError::UnexpectedEof { offset })?;
            let method = std::str::from_utf8(method_bytes)
                .map_err(|_| NefError::InvalidMethodToken { index })?
                .to_string();
            if method.starts_with('_') {
                return Err(NefError::MethodNameInvalid { name: method }.into());
            }
            offset = method_end;

            let params_bytes = bytes
                .get(offset..offset + 2)
                .ok_or(NefError::UnexpectedEof { offset })?;
            let params = u16::from_le_bytes(params_bytes.try_into().unwrap());
            offset += 2;

            let has_return_value = match bytes.get(offset) {
                Some(0) => {
                    offset += 1;
                    false
                }
                Some(1) => {
                    offset += 1;
                    true
                }
                Some(_) => {
                    return Err(NefError::InvalidMethodToken { index }.into());
                }
                None => return Err(NefError::UnexpectedEof { offset }.into()),
            };

            let call_flags = *bytes
                .get(offset)
                .ok_or(NefError::UnexpectedEof { offset })?;
            offset += 1;
            if call_flags & !super::super::CALL_FLAGS_ALLOWED_MASK != 0 {
                return Err(NefError::CallFlagsInvalid {
                    flags: call_flags,
                    allowed: super::super::CALL_FLAGS_ALLOWED_MASK,
                }
                .into());
            }

            tokens.push(MethodToken {
                hash,
                method,
                parameters_count: params,
                has_return_value,
                call_flags,
            });
        }

        Ok((tokens, offset - start))
    }
}

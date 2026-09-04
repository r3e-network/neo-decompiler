//! Shared byte limits for file and stream entry points.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// Read at most one byte past the limit so growing files and streams cannot
/// bypass the metadata check. Callers retain their own structured size errors.
pub(crate) fn read_limited<R, E>(
    reader: R,
    max: u64,
    too_large: impl FnOnce(u64, u64) -> E,
) -> Result<Vec<u8>, E>
where
    R: Read,
    E: From<io::Error>,
{
    let mut data = Vec::new();
    reader.take(max.saturating_add(1)).read_to_end(&mut data)?;
    let size = data.len() as u64;
    if size > max {
        return Err(too_large(size, max));
    }
    Ok(data)
}

/// Open once, inspect that handle, then enforce the limit while reading it.
pub(crate) fn read_file_limited<E>(
    path: &Path,
    max: u64,
    too_large: impl FnOnce(u64, u64) -> E,
) -> Result<Vec<u8>, E>
where
    E: From<io::Error>,
{
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    if size > max {
        return Err(too_large(size, max));
    }
    read_limited(file, max, too_large)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn too_large(size: u64, max: u64) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, format!("{size} > {max}"))
    }

    #[test]
    fn accepts_exact_limit_and_empty_stream() {
        assert_eq!(read_limited(&b"1234"[..], 4, too_large).unwrap(), b"1234");
        assert!(read_limited(io::empty(), 0, too_large).unwrap().is_empty());
    }

    #[test]
    fn oversized_stream_stops_after_one_lookahead_byte() {
        let mut input = Cursor::new(b"123456789");
        let error = read_limited(&mut input, 4, too_large).unwrap_err();
        assert_eq!(error.to_string(), "5 > 4");
        assert_eq!(input.position(), 5);
        let error = read_limited(io::repeat(0), 0, too_large).unwrap_err();
        assert_eq!(error.to_string(), "1 > 0");
    }

    #[test]
    fn propagates_reader_errors() {
        struct BrokenReader;
        impl Read for BrokenReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "unreadable",
                ))
            }
        }
        let error = read_limited(BrokenReader, 4, too_large).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn oversized_file_reports_metadata_size_without_buffering() {
        let file = tempfile::NamedTempFile::new().unwrap();
        file.as_file().set_len(1_000_000).unwrap();
        let error = read_file_limited(file.path(), 4, too_large).unwrap_err();
        assert_eq!(error.to_string(), "1000000 > 4");
    }
}

use std::fmt::{self, Write as _};
use std::io::{self, Read, Write as _};
use std::path::Path;

use jsonschema::validator_for;
use serde_json::Value;

use crate::error::Result;

use super::super::args::{Cli, SchemaArgs};
use super::super::schema::{SchemaKind, SchemaMetadata};

/// A maximum-size NEF can produce a report much larger than the input because
/// every instruction carries structured metadata. 128 MiB leaves ample room
/// for legitimate reports while preventing arbitrary file/stdin allocation.
const MAX_SCHEMA_INPUT_BYTES: u64 = 128 * 1024 * 1024;
const MAX_SCHEMA_ERRORS: usize = 100;
const MAX_SCHEMA_DIAGNOSTIC_BYTES: usize = 1024 * 1024;

struct BoundedDiagnostic {
    value: String,
    limit: usize,
    truncation_notice: Option<String>,
}

impl BoundedDiagnostic {
    fn new(limit: usize) -> Self {
        Self {
            value: String::new(),
            limit,
            truncation_notice: None,
        }
    }

    fn truncated(&self) -> bool {
        self.truncation_notice.is_some()
    }

    fn mark_error_limit(&mut self) {
        if self.truncation_notice.is_none() {
            self.truncation_notice = Some(format!(
                "- additional validation errors omitted (limit: {MAX_SCHEMA_ERRORS})\n"
            ));
        }
    }

    fn finish(mut self) -> String {
        if let Some(notice) = self.truncation_notice {
            while self.value.len() + notice.len() > self.limit {
                if self.value.pop().is_none() {
                    break;
                }
            }
            self.value.push_str(&notice[..notice.len().min(self.limit)]);
        }
        self.value
    }
}

impl fmt::Write for BoundedDiagnostic {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if self.truncated() {
            return Ok(());
        }
        let remaining = self.limit.saturating_sub(self.value.len());
        if value.len() <= remaining {
            self.value.push_str(value);
            return Ok(());
        }

        let mut end = remaining.min(value.len());
        while end > 0 && !value.is_char_boundary(end) {
            end -= 1;
        }
        self.value.push_str(&value[..end]);
        self.truncation_notice = Some(format!(
            "\n- validation diagnostics truncated (maximum: {MAX_SCHEMA_DIAGNOSTIC_BYTES} bytes)\n"
        ));
        Ok(())
    }
}

fn schema_input_too_large(size: u64, max: u64) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("schema validation input is {size} bytes; maximum is {max} bytes"),
    )
}

fn read_schema_input_with_limit<R: Read>(reader: R, max: u64) -> io::Result<String> {
    let data = crate::bounded_io::read_limited(reader, max, schema_input_too_large)?;
    String::from_utf8(data).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

impl Cli {
    pub(super) fn run_schema(&self, args: &SchemaArgs) -> Result<()> {
        if args.list || args.list_json {
            if args.list_json {
                let listing: Vec<_> = SchemaKind::ALL.iter().map(SchemaMetadata::report).collect();
                self.print_json(&listing)?;
            } else {
                self.write_stdout(|out| {
                    for entry in SchemaKind::ALL {
                        writeln!(
                            out,
                            "{} v{} - {}",
                            entry.kind.as_str(),
                            entry.version,
                            entry.description
                        )?;
                    }
                    Ok(())
                })?;
            }
            return Ok(());
        }

        let schema = args.schema.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "schema name is required (e.g., `schema info`) unless --list/--list-json is set",
            )
        })?;
        let entry = schema.metadata();
        let value: Value = serde_json::from_str(entry.contents).map_err(io::Error::other)?;
        if let Some(target) = args.validate.as_ref() {
            self.validate_against_schema(entry.kind.as_str(), &value, target)?;
        }
        let json = self.render_json(&value)?;
        if !args.no_print {
            self.write_stdout(|out| writeln!(out, "{json}"))?;
        }
        if let Some(path) = args.output.as_ref() {
            std::fs::write(path, &json)?;
        }
        Ok(())
    }

    fn validate_against_schema(
        &self,
        schema_name: &str,
        schema_value: &Value,
        path: &Path,
    ) -> Result<()> {
        let compiled =
            validator_for(schema_value).map_err(|err| io::Error::other(err.to_string()))?;
        let data = if path == Path::new("-") {
            read_schema_input_with_limit(io::stdin().lock(), MAX_SCHEMA_INPUT_BYTES)?
        } else {
            let data = crate::bounded_io::read_file_limited(
                path,
                MAX_SCHEMA_INPUT_BYTES,
                schema_input_too_large,
            )?;
            String::from_utf8(data)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        };
        let instance: Value = serde_json::from_str(&data)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        let mut errors = compiled.iter_errors(&instance);
        let mut buffer = BoundedDiagnostic::new(MAX_SCHEMA_DIAGNOSTIC_BYTES);
        let _ = writeln!(&mut buffer, "schema validation failed:");
        let mut error_count = 0usize;
        while error_count < MAX_SCHEMA_ERRORS && !buffer.truncated() {
            let Some(error) = errors.next() else {
                break;
            };
            error_count += 1;
            {
                let mut path = error.instance_path().to_string();
                if path.is_empty() {
                    path.push_str("<root>");
                }
                let _ = writeln!(&mut buffer, "- {path}: {error}");
            }
        }
        if error_count == MAX_SCHEMA_ERRORS && errors.next().is_some() {
            buffer.mark_error_limit();
        }
        if error_count > 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, buffer.finish()).into());
        }
        self.write_stdout(|out| {
            writeln!(
                out,
                "Validation succeeded for {} against {} schema",
                if path == Path::new("-") {
                    "stdin".into()
                } else {
                    path.display().to_string()
                },
                schema_name
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{read_schema_input_with_limit, BoundedDiagnostic};

    #[test]
    fn limited_schema_reader_accepts_the_exact_limit() {
        let value = read_schema_input_with_limit(Cursor::new(b"1234"), 4).unwrap();
        assert_eq!(value, "1234");
    }

    #[test]
    fn limited_schema_reader_rejects_one_byte_over_the_limit() {
        let error = read_schema_input_with_limit(Cursor::new(b"12345"), 4).unwrap_err();
        assert!(error.to_string().contains("maximum is 4 bytes"));
    }

    #[test]
    fn oversized_multibyte_schema_input_reports_the_byte_limit() {
        let error = read_schema_input_with_limit(Cursor::new("ééé".as_bytes()), 4).unwrap_err();
        assert!(error.to_string().contains("maximum is 4 bytes"));
    }

    #[test]
    fn diagnostic_buffer_preserves_a_bounded_truncation_notice() {
        use std::fmt::Write as _;

        let mut buffer = BoundedDiagnostic::new(128);
        let _ = write!(&mut buffer, "{}", "x".repeat(256));
        let output = buffer.finish();
        assert!(output.len() <= 128);
        assert!(output.contains("diagnostics truncated"));
    }
}

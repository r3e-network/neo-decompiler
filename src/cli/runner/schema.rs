use std::fmt::Write as _;
use std::io::{self, Read, Write as _};
use std::path::Path;

use jsonschema::JSONSchema;
use serde_json::Value;

use crate::error::Result;

use super::super::args::{Cli, SchemaArgs};
use super::super::schema::{SchemaKind, SchemaMetadata};

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
            JSONSchema::compile(schema_value).map_err(|err| io::Error::other(err.to_string()))?;
        let data = if path == Path::new("-") {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            buf
        } else {
            std::fs::read_to_string(path)?
        };
        let instance: Value = serde_json::from_str(&data)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if let Err(errors) = compiled.validate(&instance) {
            let mut buffer = String::from("schema validation failed:\n");
            for error in errors {
                let mut path = error.instance_path.to_string();
                if path.is_empty() {
                    path.push_str("<root>");
                }
                let _ = writeln!(&mut buffer, "- {path}: {error}");
            }
            return Err(io::Error::new(io::ErrorKind::InvalidData, buffer).into());
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

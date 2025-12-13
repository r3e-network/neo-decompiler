use std::path::PathBuf;

use clap::Args;

use super::super::schema::SchemaKind;

#[derive(Debug, Args)]
pub(in crate::cli) struct SchemaArgs {
    /// List available schemas
    #[arg(long, conflicts_with_all = ["schema", "output", "list_json", "validate"])]
    pub(in crate::cli) list: bool,

    /// List schemas as a JSON array
    #[arg(long, conflicts_with_all = ["schema", "output", "list", "validate"])]
    pub(in crate::cli) list_json: bool,

    /// Schema to print
    #[arg(value_enum)]
    pub(in crate::cli) schema: Option<SchemaKind>,

    /// Write the schema to a file instead of stdout
    #[arg(long, requires = "schema")]
    pub(in crate::cli) output: Option<PathBuf>,

    /// Skip printing the schema body (shorthand: --quiet)
    #[arg(long, alias = "quiet")]
    pub(in crate::cli) no_print: bool,

    /// Validate a JSON file against the specified schema
    #[arg(long, requires = "schema", conflicts_with_all = ["list", "list_json"])]
    pub(in crate::cli) validate: Option<PathBuf>,
}

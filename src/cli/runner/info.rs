use std::path::Path;

use crate::error::Result;
use crate::nef::NefParser;

use super::super::args::{Cli, InfoFormat};

mod json;
mod text;

impl Cli {
    pub(super) fn run_info(&self, path: &Path, format: InfoFormat) -> Result<()> {
        let data = Self::read_nef_bytes(path)?;
        let nef = NefParser::new().parse(&data)?;
        let manifest = self.load_manifest(path)?;
        let manifest_path = self.resolve_manifest_path(path);

        match format {
            InfoFormat::Text => {
                self.print_info_text(path, &nef, manifest.as_ref(), manifest_path.as_ref())
            }
            InfoFormat::Json => {
                self.print_info_json(path, &nef, manifest.as_ref(), manifest_path.as_ref())
            }
        }
    }
}

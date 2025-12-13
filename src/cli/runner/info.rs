use std::path::PathBuf;

use crate::error::Result;
use crate::manifest::ContractManifest;
use crate::nef::NefParser;

use super::super::args::{Cli, InfoFormat};

mod json;
mod text;

impl Cli {
    pub(super) fn run_info(&self, path: &PathBuf, format: InfoFormat) -> Result<()> {
        let data = std::fs::read(path)?;
        let nef = NefParser::new().parse(&data)?;
        let manifest_path = self.resolve_manifest_path(path);
        let manifest = match manifest_path.as_ref() {
            Some(p) => Some(ContractManifest::from_file(p)?),
            None => None,
        };

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

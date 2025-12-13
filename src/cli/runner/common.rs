use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::Result;

use super::super::args::{CatalogKind, Cli};
use super::super::catalog::CatalogReport;

impl Cli {
    pub(super) fn resolve_manifest_path(&self, nef_path: &Path) -> Option<PathBuf> {
        if let Some(path) = &self.manifest {
            return Some(path.clone());
        }

        let mut candidate = nef_path.to_path_buf();
        candidate.set_extension("manifest.json");
        if candidate.exists() {
            return Some(candidate);
        }

        None
    }

    pub(super) fn render_json<T: Serialize>(&self, value: &T) -> io::Result<String> {
        if self.json_compact {
            serde_json::to_string(value)
        } else {
            serde_json::to_string_pretty(value)
        }
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }

    pub(super) fn print_json<T: Serialize>(&self, value: &T) -> Result<()> {
        let json = self.render_json(value)?;
        println!("{json}");
        Ok(())
    }

    pub(super) fn print_catalog_json<T: Serialize>(
        &self,
        kind: CatalogKind,
        entries: Vec<T>,
    ) -> Result<()> {
        let report = CatalogReport {
            kind: kind.as_str(),
            count: entries.len(),
            entries,
        };
        self.print_json(&report)
    }
}

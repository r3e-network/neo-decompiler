use std::fmt;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::disassembler::UnknownHandling;
use crate::error::Result;
use crate::manifest::ContractManifest;
use crate::util;

use super::super::args::{CatalogKind, Cli};
use super::super::catalog::CatalogReport;

impl Cli {
    pub(super) fn write_stdout<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&mut io::StdoutLock<'_>) -> io::Result<()>,
    {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        match f(&mut handle) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

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
        .map_err(io::Error::other)
    }

    pub(super) fn print_json<T: Serialize>(&self, value: &T) -> Result<()> {
        let json = self.render_json(value)?;
        self.write_stdout(|out| writeln!(out, "{json}"))
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

    /// Read a NEF file with the same byte limit as the library entry points.
    pub(super) fn read_nef_bytes(path: &Path) -> Result<Vec<u8>> {
        crate::nef::read_nef_file(path)
    }

    /// Convert a `--fail-on-unknown-opcodes` flag into [`UnknownHandling`].
    pub(super) fn unknown_handling(fail_on_unknown: bool) -> UnknownHandling {
        if fail_on_unknown {
            UnknownHandling::Error
        } else {
            UnknownHandling::Permit
        }
    }

    /// Resolve and load the contract manifest, respecting `--strict-manifest`.
    pub(super) fn load_manifest(&self, nef_path: &Path) -> Result<Option<ContractManifest>> {
        match self.resolve_manifest_path(nef_path) {
            Some(p) => Ok(Some(if self.strict_manifest {
                ContractManifest::from_file_strict(&p)?
            } else {
                ContractManifest::from_file(&p)?
            })),
            None => Ok(None),
        }
    }

    /// Write a `Warnings:` block to `out` if `warnings` is non-empty.
    pub(super) fn write_warnings<W: fmt::Display>(
        out: &mut impl io::Write,
        warnings: &[W],
    ) -> io::Result<()> {
        if !warnings.is_empty() {
            writeln!(out)?;
            writeln!(out, "Warnings:")?;
            for warning in warnings {
                writeln!(out, "- {}", util::escape_visible_text(&warning.to_string()))?;
            }
        }
        Ok(())
    }
}

use std::path::{Path, PathBuf};

use std::io::Write as _;

use crate::error::Result;
use crate::manifest::ContractManifest;
use crate::nef::NefFile;
use crate::util;

use super::super::super::args::Cli;
use super::super::super::reports;

impl Cli {
    pub(super) fn print_info_text(
        &self,
        path: &Path,
        nef: &NefFile,
        manifest: Option<&ContractManifest>,
        manifest_path: Option<&PathBuf>,
    ) -> Result<()> {
        self.write_stdout(|out| {
            writeln!(out, "File: {}", path.display())?;
            writeln!(out, "Compiler: {}", nef.header.compiler)?;
            if !nef.header.source.is_empty() {
                writeln!(out, "Source: {}", nef.header.source)?;
            }
            writeln!(out, "Script length: {} bytes", nef.script.len())?;
            let script_hash = nef.script_hash();
            writeln!(out, "Script hash (LE): {}", util::format_hash(&script_hash))?;
            writeln!(
                out,
                "Script hash (BE): {}",
                util::format_hash_be(&script_hash)
            )?;
            writeln!(out, "Method tokens: {}", nef.method_tokens.len())?;
            if !nef.method_tokens.is_empty() {
                writeln!(out, "Method token entries:")?;
                for (index, token) in nef.method_tokens.iter().enumerate() {
                    writeln!(
                        out,
                        "    {}",
                        reports::format_method_token_line(index, token)
                    )?;
                }
            }
            writeln!(out, "Checksum: 0x{:08X}", nef.checksum)?;

            if let Some(manifest) = manifest {
                writeln!(out, "Manifest contract: {}", manifest.name)?;
                if !manifest.supported_standards.is_empty() {
                    writeln!(
                        out,
                        "Supported standards: {}",
                        manifest.supported_standards.join(", ")
                    )?;
                }
                writeln!(
                    out,
                    "ABI methods: {} events: {}",
                    manifest.abi.methods.len(),
                    manifest.abi.events.len()
                )?;
                writeln!(
                    out,
                    "Features: storage={} payable={}",
                    manifest.features.storage, manifest.features.payable
                )?;
                if !manifest.groups.is_empty() {
                    writeln!(out, "Groups:")?;
                    for group in &manifest.groups {
                        writeln!(
                            out,
                            "    - pubkey={} signature={}",
                            group.pubkey, group.signature
                        )?;
                    }
                }
                if !manifest.permissions.is_empty() {
                    writeln!(out, "Permissions:")?;
                    for permission in &manifest.permissions {
                        writeln!(
                            out,
                            "    - contract={} methods={}",
                            permission.contract.describe(),
                            permission.methods.describe()
                        )?;
                    }
                }
                if let Some(trusts) = manifest.trusts.as_ref() {
                    writeln!(out, "Trusts: {}", trusts.describe())?;
                }
                if let Some(path) = manifest_path {
                    writeln!(out, "Manifest path: {}", path.display())?;
                }
            }
            Ok(())
        })
    }
}

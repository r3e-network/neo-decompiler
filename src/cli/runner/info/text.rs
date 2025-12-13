use std::path::{Path, PathBuf};

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
        println!("File: {}", path.display());
        println!("Compiler: {}", nef.header.compiler);
        if !nef.header.source.is_empty() {
            println!("Source: {}", nef.header.source);
        }
        println!("Script length: {} bytes", nef.script.len());
        let script_hash = nef.script_hash();
        println!("Script hash (LE): {}", util::format_hash(&script_hash));
        println!("Script hash (BE): {}", util::format_hash_be(&script_hash));
        println!("Method tokens: {}", nef.method_tokens.len());
        if !nef.method_tokens.is_empty() {
            println!("Method token entries:");
            for (index, token) in nef.method_tokens.iter().enumerate() {
                println!("    {}", reports::format_method_token_line(index, token));
            }
        }
        println!("Checksum: 0x{:08X}", nef.checksum);

        if let Some(manifest) = manifest {
            println!("Manifest contract: {}", manifest.name);
            if !manifest.supported_standards.is_empty() {
                println!(
                    "Supported standards: {}",
                    manifest.supported_standards.join(", ")
                );
            }
            println!(
                "ABI methods: {} events: {}",
                manifest.abi.methods.len(),
                manifest.abi.events.len()
            );
            println!(
                "Features: storage={} payable={}",
                manifest.features.storage, manifest.features.payable
            );
            if !manifest.groups.is_empty() {
                println!("Groups:");
                for group in &manifest.groups {
                    println!(
                        "    - pubkey={} signature={}",
                        group.pubkey, group.signature
                    );
                }
            }
            if !manifest.permissions.is_empty() {
                println!("Permissions:");
                for permission in &manifest.permissions {
                    println!(
                        "    - contract={} methods={}",
                        permission.contract.describe(),
                        permission.methods.describe()
                    );
                }
            }
            if let Some(trusts) = manifest.trusts.as_ref() {
                println!("Trusts: {}", trusts.describe());
            }
            if let Some(path) = manifest_path {
                println!("Manifest path: {}", path.display());
            }
        }
        Ok(())
    }
}

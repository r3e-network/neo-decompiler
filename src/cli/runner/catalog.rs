use crate::error::Result;

use std::io::Write as _;

use super::super::args::{CatalogArgs, CatalogFormat, CatalogKind, Cli};
use super::super::catalog as catalog_data;

impl Cli {
    pub(super) fn run_catalog(&self, args: &CatalogArgs) -> Result<()> {
        match args.kind {
            CatalogKind::Syscalls => self.print_syscall_catalog(args.format),
            CatalogKind::NativeContracts => self.print_native_contract_catalog(args.format),
            CatalogKind::Opcodes => self.print_opcode_catalog(args.format),
        }
    }

    fn print_syscall_catalog(&self, format: CatalogFormat) -> Result<()> {
        let entries = catalog_data::build_syscall_catalog_entries();
        match format {
            CatalogFormat::Text => {
                self.write_stdout(|out| {
                    writeln!(out, "{} syscalls bundled", entries.len())?;
                    writeln!(out)?;
                    for entry in &entries {
                        writeln!(out, "{} ({})", entry.name, entry.hash)?;
                        writeln!(out, "    handler: {}", entry.handler)?;
                        writeln!(out, "    price: {}", entry.price)?;
                        writeln!(out, "    call_flags: {}", entry.call_flags)?;
                        writeln!(out, "    returns_value: {}", entry.returns_value)?;
                        writeln!(out)?;
                    }
                    Ok(())
                })?;
            }
            CatalogFormat::Json => {
                self.print_catalog_json(CatalogKind::Syscalls, entries)?;
            }
        }
        Ok(())
    }

    fn print_native_contract_catalog(&self, format: CatalogFormat) -> Result<()> {
        let entries = catalog_data::build_native_contract_catalog_entries();
        match format {
            CatalogFormat::Text => {
                self.write_stdout(|out| {
                    writeln!(out, "{} native contracts bundled", entries.len())?;
                    writeln!(out)?;
                    for entry in &entries {
                        writeln!(out, "{} ({})", entry.name, entry.script_hash_le)?;
                        writeln!(out, "    script_hash_be: {}", entry.script_hash_be)?;
                        if entry.methods.is_empty() {
                            writeln!(out, "    methods: (none)")?;
                        } else {
                            writeln!(out, "    methods: {}", entry.methods.join(", "))?;
                        }
                        writeln!(out)?;
                    }
                    Ok(())
                })?;
            }
            CatalogFormat::Json => {
                self.print_catalog_json(CatalogKind::NativeContracts, entries)?;
            }
        }
        Ok(())
    }

    fn print_opcode_catalog(&self, format: CatalogFormat) -> Result<()> {
        let entries = catalog_data::build_opcode_catalog_entries();
        match format {
            CatalogFormat::Text => {
                self.write_stdout(|out| {
                    writeln!(out, "{} opcodes bundled", entries.len())?;
                    writeln!(out)?;
                    for entry in &entries {
                        writeln!(out, "{} ({})", entry.mnemonic, entry.byte)?;
                        writeln!(out, "    operand: {}", entry.operand_encoding)?;
                        writeln!(out)?;
                    }
                    Ok(())
                })?;
            }
            CatalogFormat::Json => {
                self.print_catalog_json(CatalogKind::Opcodes, entries)?;
            }
        }
        Ok(())
    }
}

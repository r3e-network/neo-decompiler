use crate::error::Result;

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
                println!("{} syscalls bundled", entries.len());
                println!();
                for entry in &entries {
                    println!("{} ({})", entry.name, entry.hash);
                    println!("    handler: {}", entry.handler);
                    println!("    price: {}", entry.price);
                    println!("    call_flags: {}", entry.call_flags);
                    println!("    returns_value: {}", entry.returns_value);
                    println!();
                }
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
                println!("{} native contracts bundled", entries.len());
                println!();
                for entry in &entries {
                    println!("{} ({})", entry.name, entry.script_hash_le);
                    println!("    script_hash_be: {}", entry.script_hash_be);
                    if entry.methods.is_empty() {
                        println!("    methods: (none)");
                    } else {
                        println!("    methods: {}", entry.methods.join(", "));
                    }
                    println!();
                }
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
                println!("{} opcodes bundled", entries.len());
                println!();
                for entry in &entries {
                    println!("{} ({})", entry.mnemonic, entry.byte);
                    println!("    operand: {}", entry.operand_encoding);
                    println!();
                }
            }
            CatalogFormat::Json => {
                self.print_catalog_json(CatalogKind::Opcodes, entries)?;
            }
        }
        Ok(())
    }
}

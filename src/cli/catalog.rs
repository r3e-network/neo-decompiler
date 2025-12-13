use serde::Serialize;

use crate::instruction::OpCode;
use crate::native_contracts;
use crate::syscalls;
use crate::util;

#[derive(Serialize)]
pub(super) struct CatalogReport<T> {
    pub(super) kind: &'static str,
    pub(super) count: usize,
    pub(super) entries: Vec<T>,
}

#[derive(Serialize)]
pub(super) struct SyscallCatalogEntry {
    pub(super) name: String,
    pub(super) hash: String,
    pub(super) handler: String,
    pub(super) price: String,
    pub(super) call_flags: String,
    pub(super) returns_value: bool,
}

#[derive(Serialize)]
pub(super) struct NativeContractCatalogEntry {
    pub(super) name: String,
    pub(super) script_hash_le: String,
    pub(super) script_hash_be: String,
    pub(super) methods: Vec<String>,
}

#[derive(Serialize)]
pub(super) struct OpcodeCatalogEntry {
    pub(super) mnemonic: String,
    pub(super) byte: String,
    pub(super) operand_encoding: String,
}

pub(super) fn build_syscall_catalog_entries() -> Vec<SyscallCatalogEntry> {
    syscalls::all()
        .iter()
        .map(|info| SyscallCatalogEntry {
            name: info.name.to_string(),
            hash: format!("0x{:08X}", info.hash),
            handler: info.handler.to_string(),
            price: info.price.to_string(),
            call_flags: info.call_flags.to_string(),
            returns_value: info.returns_value,
        })
        .collect()
}

pub(super) fn build_native_contract_catalog_entries() -> Vec<NativeContractCatalogEntry> {
    native_contracts::all()
        .iter()
        .map(|info| NativeContractCatalogEntry {
            name: info.name.to_string(),
            script_hash_le: util::format_hash(&info.script_hash),
            script_hash_be: util::format_hash_be(&info.script_hash),
            methods: info
                .methods
                .iter()
                .map(|method| (*method).to_string())
                .collect(),
        })
        .collect()
}

pub(super) fn build_opcode_catalog_entries() -> Vec<OpcodeCatalogEntry> {
    let mut opcodes = OpCode::all_known();
    opcodes.sort_by_key(|opcode| opcode.byte());
    opcodes
        .into_iter()
        .map(|opcode| OpcodeCatalogEntry {
            mnemonic: opcode.mnemonic().to_string(),
            byte: format!("0x{:02X}", opcode.byte()),
            operand_encoding: format!("{:?}", opcode.operand_encoding()),
        })
        .collect()
}

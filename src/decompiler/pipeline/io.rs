use std::path::Path;

use crate::disassembler::DisassemblyOutput;
use crate::error::Result;
use crate::manifest::ContractManifest;
use crate::nef::read_nef_file;

use super::super::{Decompilation, OutputFormat};
use super::Decompiler;

impl Decompiler {
    pub(super) fn io_decompile_file<P: AsRef<Path>>(&self, path: P) -> Result<Decompilation> {
        let data = read_nef_file(path.as_ref())?;
        self.decompile_bytes(&data)
    }

    pub(super) fn io_disassemble_file<P: AsRef<Path>>(&self, path: P) -> Result<DisassemblyOutput> {
        let data = read_nef_file(path.as_ref())?;
        self.disassemble_bytes(&data)
    }

    pub(super) fn io_decompile_file_with_manifest<P, Q>(
        &self,
        nef_path: P,
        manifest_path: Option<Q>,
        output_format: OutputFormat,
    ) -> Result<Decompilation>
    where
        P: AsRef<Path>,
        Q: AsRef<Path>,
    {
        let nef_path = nef_path.as_ref();
        let data = read_nef_file(nef_path)?;
        let manifest = match manifest_path {
            Some(path) => Some(ContractManifest::from_file(path)?),
            None => None,
        };
        self.decompile_bytes_with_manifest(&data, manifest, output_format)
    }
}

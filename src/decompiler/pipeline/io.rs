use std::fs;
use std::path::Path;

use crate::disassembler::DisassemblyOutput;
use crate::error::{NefError, Result};
use crate::manifest::ContractManifest;

use super::super::{Decompilation, OutputFormat, MAX_NEF_FILE_SIZE};
use super::Decompiler;

fn read_nef_file(path: &Path) -> Result<Vec<u8>> {
    let size = fs::metadata(path)?.len();
    if size > MAX_NEF_FILE_SIZE {
        return Err(NefError::FileTooLarge {
            size,
            max: MAX_NEF_FILE_SIZE,
        }
        .into());
    }
    Ok(fs::read(path)?)
}

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

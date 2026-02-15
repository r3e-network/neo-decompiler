use std::io::Write as _;
use std::path::PathBuf;

use crate::decompiler::Decompiler;
use crate::error::Result;

use super::super::args::Cli;

impl Cli {
    pub(super) fn run_cfg(&self, path: &PathBuf, fail_on_unknown_opcodes: bool) -> Result<()> {
        let handling = Self::unknown_handling(fail_on_unknown_opcodes);
        let decompiler = Decompiler::with_unknown_handling(handling);
        let result = decompiler.decompile_file(path)?;

        self.write_stdout(|out| out.write_all(result.cfg_to_dot().as_bytes()))
    }
}

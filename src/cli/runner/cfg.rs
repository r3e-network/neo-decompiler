use std::io::Write as _;
use std::path::Path;

use crate::decompiler::{Decompiler, OutputFormat};
use crate::error::Result;

use super::super::args::Cli;

impl Cli {
    pub(super) fn run_cfg(&self, path: &Path, fail_on_unknown_opcodes: bool) -> Result<()> {
        let handling = Self::unknown_handling(fail_on_unknown_opcodes);
        let decompiler = Decompiler::with_unknown_handling(handling);
        // Load the manifest when present so the CFG's graph label
        // shows the contract name alongside the script hash —
        // useful when dumping multiple contracts to a single
        // graphviz canvas.
        let data = Self::read_nef_bytes(path)?;
        let manifest = self.load_manifest(path)?;
        // CFG is built regardless of output format; pick the cheapest
        // (`Pseudocode`) so we don't pay for high-level / C# rendering
        // we'll never use.
        let result =
            decompiler.decompile_bytes_with_manifest(&data, manifest, OutputFormat::Pseudocode)?;

        self.write_stdout(|out| out.write_all(result.cfg_to_dot().as_bytes()))
    }
}

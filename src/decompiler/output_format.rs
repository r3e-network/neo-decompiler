#[cfg(feature = "cli")]
use clap::ValueEnum;

/// Select which decompiler outputs should be generated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(ValueEnum))]
pub enum OutputFormat {
    /// Emit pseudocode output.
    Pseudocode,
    /// Emit a higher-level structured output.
    HighLevel,
    /// Emit C# source code output.
    CSharp,
    /// Emit all supported outputs.
    #[default]
    All,
}

impl OutputFormat {
    pub(super) fn wants_pseudocode(self) -> bool {
        matches!(self, OutputFormat::Pseudocode | OutputFormat::All)
    }

    pub(super) fn wants_high_level(self) -> bool {
        matches!(self, OutputFormat::HighLevel | OutputFormat::All)
    }

    pub(super) fn wants_csharp(self) -> bool {
        matches!(self, OutputFormat::CSharp | OutputFormat::All)
    }
}

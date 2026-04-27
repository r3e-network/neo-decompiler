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
    // Override clap's default kebab-case derivation (`c-sharp`) so the
    // CLI accepts the same `csharp` token as `--format csharp`. The
    // legacy `c-sharp` form is kept as an alias for back-compat with
    // any scripts that pinned the old spelling.
    #[cfg_attr(feature = "cli", value(name = "csharp", alias = "c-sharp"))]
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

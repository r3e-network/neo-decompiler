use clap::ValueEnum;

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub(in crate::cli) enum DecompileFormat {
    Pseudocode,
    HighLevel,
    Both,
    #[default]
    Csharp,
    Json,
    /// Structured IR view (CFG → ir::ControlFlow), the Phase-4 spine path.
    Ir,
    /// Optimized SSA view (def/use, phi, constant folding/DCE).
    Ssa,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub(in crate::cli) enum InfoFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub(in crate::cli) enum DisasmFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub(in crate::cli) enum TokensFormat {
    #[default]
    Text,
    Json,
}

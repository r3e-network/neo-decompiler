use clap::ValueEnum;

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub(in crate::cli) enum DecompileFormat {
    Pseudocode,
    #[default]
    HighLevel,
    Both,
    Csharp,
    Json,
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

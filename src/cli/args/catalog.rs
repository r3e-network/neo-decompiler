use clap::{Args, ValueEnum};

#[derive(Debug, Args)]
pub(in crate::cli) struct CatalogArgs {
    /// Which metadata table to print
    #[arg(value_enum)]
    pub(in crate::cli) kind: CatalogKind,

    /// Choose the output view
    #[arg(long, value_enum, default_value_t = CatalogFormat::Text)]
    pub(in crate::cli) format: CatalogFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(in crate::cli) enum CatalogKind {
    Syscalls,
    NativeContracts,
    Opcodes,
}

impl CatalogKind {
    pub(in crate::cli) fn as_str(self) -> &'static str {
        match self {
            CatalogKind::Syscalls => "syscalls",
            CatalogKind::NativeContracts => "native-contracts",
            CatalogKind::Opcodes => "opcodes",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub(in crate::cli) enum CatalogFormat {
    #[default]
    Text,
    Json,
}

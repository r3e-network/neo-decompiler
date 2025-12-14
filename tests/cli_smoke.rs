#![cfg(feature = "cli")]

#[path = "cli_smoke/common.rs"]
mod common;

#[path = "cli_smoke/catalog.rs"]
mod catalog;

#[path = "cli_smoke/cfg.rs"]
mod cfg;

#[path = "cli_smoke/decompile.rs"]
mod decompile;

#[path = "cli_smoke/disasm.rs"]
mod disasm;

#[path = "cli_smoke/info.rs"]
mod info;

#[path = "cli_smoke/schema.rs"]
mod schema;

#[path = "cli_smoke/tokens.rs"]
mod tokens;

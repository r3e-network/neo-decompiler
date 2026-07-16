use std::fs;
use std::path::Path;

use neo_decompiler::{ContractManifest, Decompiler, OutputFormat};

fn method_block<'a>(text: &'a str, start_marker: &str, next_marker: &str) -> &'a str {
    let start = text
        .find(start_marker)
        .unwrap_or_else(|| panic!("missing marker `{start_marker}`"));
    let end = text[start..]
        .find(next_marker)
        .map(|relative| start + relative)
        .unwrap_or(text.len());
    &text[start..end]
}

#[path = "parity/calls_helpers.rs"]
mod calls_helpers;
#[path = "parity/contracts.rs"]
mod contracts;
#[path = "parity/properties.rs"]
mod properties;
#[path = "parity/recursion.rs"]
mod recursion;
#[path = "parity/stack_shapes.rs"]
mod stack_shapes;
#[path = "parity/switches.rs"]
mod switches;

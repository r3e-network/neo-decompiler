//! Command line interface for inspecting and decompiling Neo N3 NEF files.

mod args;
mod catalog;
mod reports;
mod runner;
mod schema;

pub use args::Cli;

//! Neo N3 Decompiler CLI Application
//! 
//! A comprehensive command-line interface for the Neo N3 smart contract decompiler.
//! Provides multiple analysis modes, output formats, and development tools.

use clap::Parser;
use std::process;
use tracing::{error, Level};
use tracing_subscriber::{fmt, EnvFilter};

use neo_decompiler::cli::Cli;

fn main() {
    let cli = Cli::parse();

    // Initialize logging with enhanced formatting and filtering
    let log_level = match cli.verbose {
        0 => Level::ERROR,
        1 => Level::WARN,
        2 => Level::INFO,
        3 => Level::DEBUG,
        _ => Level::TRACE,
    };

    let env_filter = EnvFilter::from_default_env()
        .add_directive(format!("neo_decompiler={}", log_level).parse().unwrap());

    fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_level(true)
        .with_thread_ids(false)
        .init();

    // Execute the command and handle errors gracefully
    if let Err(e) = cli.run() {
        error!("Command failed: {}", e);
        
        // Print cause chain for better debugging
        let mut cause = e.source();
        while let Some(err) = cause {
            error!("  Caused by: {}", err);
            cause = err.source();
        }
        
        process::exit(1);
    }
}
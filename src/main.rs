use clap::Parser;

use neo_decompiler::cli::Cli;

fn main() {
    if let Err(err) = Cli::parse().run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

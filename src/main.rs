use std::path::{self, PathBuf};

use clap::Parser;

/// Compiler for the rove programming language.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    input: PathBuf,
    #[clap(short, long)]
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    rove::compile(path::absolute(&cli.input)?, path::absolute(&cli.output)?)
        .expect("to compile fully");

    Ok(())
}

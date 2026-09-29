use std::path::{self, PathBuf};

use clap::Parser;

/// Compiler for the rove programming language.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    input: PathBuf,
    #[clap(short, long)]
    output: Option<PathBuf>,
    /// Debug artifacts to write next to the input file as `__<file>.<ext>`.
    /// Can be repeated or comma separated, e.g. `--emit ast,clif`.
    #[clap(long, value_enum, value_delimiter = ',')]
    emit: Vec<rove::Emit>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    rove::compileq!(
        path::absolute(&cli.input)?,
        path::absolute(&cli.output.unwrap_or_else(|| cli.input.with_extension("")))?,
        &cli.emit
    )
    .map_err(|_| "unable to compile".to_string())?;

    Ok(())
}

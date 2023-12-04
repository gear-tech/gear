//! mini-program for publishing packages to crates.io.

use anyhow::Result;
use clap::Parser;
use crates_io_manager::Publisher;

/// The command to run.
#[derive(Clone, Debug, Parser)]
enum Command {
    /// Check packages that to be published.
    Check,
    /// Publish packages.
    Publish,
}

/// Gear crates-io manager command line interface
///
/// NOTE: this binary should not be used locally
/// but run in CI.
#[derive(Debug, Parser)]
pub struct Opt {
    #[clap(subcommand)]
    command: Command,
}

fn main() -> Result<()> {
    let Opt { command } = Opt::parse();

    let publisher = Publisher::new()?.build()?;
    match command {
        Command::Check => publisher.check(),
        Command::Publish => publisher.publish(),
    }
}

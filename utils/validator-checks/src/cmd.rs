use clap::Parser;

/// Entrypoint of cli `validator-checks`
#[derive(Debug, Parser)]
pub struct Opt {
    /// The network to be checked.
    endpoint: String,
    /// Validators to be checked.
    ///
    /// If none provided, will check all authorities.
    validators: Vec<String>,
    /// Check if validators produce blocks.
    #[arg(short, long)]
    produce_blocks: bool,
}

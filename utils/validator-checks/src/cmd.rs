use crate::{listener::Listener, result::Result};
use clap::Parser;

/// Entrypoint of cli `validator-checks`
#[derive(Debug, Parser)]
pub struct Opt {
    /// The network to be checked.
    pub endpoint: Option<String>,
    /// Validators to be checked.
    ///
    /// If none provided, will check all authorities.
    pub validators: Vec<String>,
    /// Check if validators produce blocks.
    #[arg(short, long)]
    pub produce_blocks: bool,
}

impl Opt {
    /// Run validator checks.
    pub async fn run(self) -> Result<()> {
        Listener::new(self).await?.check().await?;
        Ok(())
    }
}

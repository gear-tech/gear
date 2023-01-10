use crate::{listener::Listener, result::Result};
use clap::Parser;
use env_logger::{Builder, Env};

/// Entrypoint of cli `validator-checks`
#[derive(Debug, Parser)]
pub struct Opt {
    /// Validators to be checked.
    ///
    /// If none provided, will check all authorities.
    pub validators: Vec<String>,
    /// The network to be checked.
    #[arg(short, long)]
    pub endpoint: Option<String>,
    /// Check if validators produce blocks.
    #[arg(short, long)]
    pub block_production: bool,
    /// Enable verbose logs.
    #[arg(short, long)]
    pub verbose: bool,
}

impl Opt {
    /// Run validator checks.
    pub async fn run(self) -> Result<()> {
        let mut builder = if self.verbose {
            Builder::from_env(Env::default().default_filter_or("validator_checks=debug"))
        } else {
            Builder::from_env(Env::default().default_filter_or("validator_checks=info"))
        };
        builder.try_init()?;

        Listener::new(self).await?.check().await?;
        Ok(())
    }
}

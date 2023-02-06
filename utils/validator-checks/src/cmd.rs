use crate::{listener::Listener, result::Result};
use clap::Parser;
use env_logger::{Builder, Env};

/// Entrypoint of cli `validator-checks`
#[derive(Debug, Parser)]
pub struct Opt {
    /// Validators to be checked.
    ///
    /// If nothing provided, will check all authorities.
    pub validators: Vec<String>,
    /// The network to be checked.
    #[arg(short, long)]
    pub endpoint: Option<String>,
    /// Timeout of all checks. ( milliseconds )
    #[arg(short, long, default_value = "600000")]
    pub timeout: u128,
}

impl Opt {
    /// Run validator checks.
    pub async fn run(self) -> Result<()> {
        Builder::from_env(Env::default().default_filter_or("gear_validator_checks=info"))
            .try_init()?;

        Listener::new(self).await?.check().await?;
        Ok(())
    }
}

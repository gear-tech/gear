use crate::{
    listener::Listener,
    result::{Error, Result},
};
use clap::Parser;
use tracing_subscriber::EnvFilter;

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
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::builder()
                    .with_default_directive(
                        "gear_validator_checks=info"
                            .parse()
                            .map_err(Error::EnvFilter)?,
                    )
                    .from_env_lossy(),
            )
            .try_init()
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        Listener::new(self).await?.check().await?;
        Ok(())
    }
}

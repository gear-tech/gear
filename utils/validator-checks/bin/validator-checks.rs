use clap::Parser;
use color_eyre::eyre::Result;
use validator_checks::Opt;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    if let Err(e) = Opt::parse().run().await {
        log::error!("{:?}", e);
    }

    Ok(())
}

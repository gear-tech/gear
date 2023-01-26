use clap::Parser;
use color_eyre::eyre::Result;
use gear_validator_checks::Opt;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    if let Err(e) = Opt::parse().run().await {
        log::error!("{:?}", e);
        std::process::exit(1);
    }

    Ok(())
}

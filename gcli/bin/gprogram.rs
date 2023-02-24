//! gear command entry
use color_eyre::eyre::Result;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    if let Err(e) = gear_program::cmd::Opt::run().await {
        log::error!("{}", e);
    }

    Ok(())
}

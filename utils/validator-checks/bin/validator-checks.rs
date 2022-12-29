use clap::Parser;
use color_eyre::eyre::Result;
use validator_checks::Opt;

fn main() -> Result<()> {
    color_eyre::install()?;

    println!("{:?}", Opt::parse());

    Ok(())
}

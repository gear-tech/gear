//! commands
use crate::{Registry, Result};
use structopt::StructOpt;

// mod deploy;
// mod login;
mod new;
mod update;

#[derive(Debug, StructOpt)]
pub enum Command {
    New(new::New),
    Update(update::Update),
}

#[derive(Debug, StructOpt)]
#[structopt(name = "gear-program")]
pub struct Opt {
    /// Enable debug logs
    #[structopt(short, long)]
    pub debug: bool,
    #[structopt(subcommand)]
    pub command: Command,
}

impl Opt {
    /// run program
    pub async fn run() -> Result<()> {
        Registry::default().init().await?;

        let opt = Opt::from_args();

        match opt.command {
            Command::New(new) => new.exec()?,
            Command::Update(update) => update.exec().await?,
        }

        Ok(())
    }
}

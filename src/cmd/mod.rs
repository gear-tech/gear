//! commands
use crate::Result;
use structopt::StructOpt;

mod deploy;
mod info;
mod login;
mod new;
mod transfer;
mod update;

#[derive(Debug, StructOpt)]
pub enum Command {
    Deploy(deploy::Deploy),
    Login(login::Login),
    New(new::New),
    Update(update::Update),
    Transfer(transfer::Transfer),
    Info(info::Info),
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
        let opt = Opt::from_args();

        match opt.command {
            Command::Login(login) => login.exec()?,
            Command::New(new) => new.exec().await?,
            Command::Deploy(deploy) => deploy.exec().await?,
            Command::Update(update) => update.exec().await?,
            Command::Transfer(transfer) => transfer.exec().await?,
            Command::Info(info) => info.exec().await?,
        }

        Ok(())
    }
}

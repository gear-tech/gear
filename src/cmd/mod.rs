//! commands
use crate::Result;
use structopt::StructOpt;

// mod deploy;
// mod login;
mod login;
mod new;
mod submit;
mod update;

#[derive(Debug, StructOpt)]
pub enum Command {
    Login(login::Login),
    New(new::New),
    Update(update::Update),
    Submit(submit::Submit),
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
            Command::New(new) => new.exec()?,
            Command::Submit(submit) => submit.exec().await?,
            Command::Update(update) => update.exec().await?,
        }

        Ok(())
    }
}

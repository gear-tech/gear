//! commands
use crate::{api::Api, Result};
use structopt::StructOpt;

mod claim;
mod deploy;
mod info;
mod login;
mod meta;
mod new;
mod reply;
mod send;
mod submit;
mod transfer;
mod update;

#[derive(Debug, StructOpt)]
pub enum Command {
    Claim(claim::Claim),
    Deploy(deploy::Deploy),
    Info(info::Info),
    Login(login::Login),
    Meta(meta::Meta),
    New(new::New),
    Reply(reply::Reply),
    Send(send::Send),
    Submit(submit::Submit),
    Transfer(transfer::Transfer),
    Update(update::Update),
}

#[derive(Debug, StructOpt)]
#[structopt(name = "gear-program")]
pub struct Opt {
    /// Commands.
    #[structopt(subcommand)]
    pub command: Command,
    /// Enable debug logs.
    #[structopt(short, long)]
    pub debug: bool,
    /// Gear node rpc endpoint.
    #[structopt(short, long)]
    pub endpoint: Option<String>,
    /// Password of the signer account.
    #[structopt(short, long)]
    pub passwd: Option<String>,
}

impl Opt {
    /// run program
    pub async fn run() -> Result<()> {
        Opt::from_args().exec().await?;

        Ok(())
    }

    /// Generate api from options.
    pub async fn api(&self) -> Result<Api> {
        Api::new(self.endpoint.as_deref(), self.passwd.as_deref()).await
    }

    /// Execute command.
    pub async fn exec(&self) -> Result<()> {
        match &self.command {
            Command::Claim(claim) => claim.exec(self.api().await?).await?,
            Command::Deploy(deploy) => deploy.exec(self.api().await?).await?,
            Command::Info(info) => info.exec(self.api().await?).await?,
            Command::Login(login) => login.exec()?,
            Command::Meta(meta) => meta.exec()?,
            Command::New(new) => new.exec().await?,
            Command::Reply(reply) => reply.exec(self.api().await?).await?,
            Command::Send(send) => send.exec(self.api().await?).await?,
            Command::Submit(submit) => submit.exec(self.api().await?).await?,
            Command::Transfer(transfer) => transfer.exec(self.api().await?).await?,
            Command::Update(update) => update.exec().await?,
        }

        Ok(())
    }
}

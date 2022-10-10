//! commands
#![cfg(feature = "cli")]
use crate::{api::Api, result::Result};
use clap::Parser;
use env_logger::{Builder, Env};
use log::LevelFilter;

pub mod claim;
pub mod create;
pub mod info;
pub mod key;
pub mod login;
pub mod meta;
pub mod new;
pub mod program;
pub mod reply;
pub mod send;
pub mod transfer;
pub mod update;
pub mod upload;
pub mod upload_program;

/// Commands of cli `gear`
#[derive(Debug, Parser)]
pub enum Command {
    Claim(claim::Claim),
    Create(create::Create),
    Info(info::Info),
    Key(key::Key),
    Login(login::Login),
    Meta(meta::Meta),
    New(new::New),
    Program(program::Program),
    Reply(reply::Reply),
    Send(send::Send),
    Upload(upload::Upload),
    UploadProgram(upload_program::UploadProgram),
    Transfer(transfer::Transfer),
    Update(update::Update),
}

/// Entrypoint of cli `gear`
#[derive(Debug, Parser)]
#[clap(name = "gear-program")]
pub struct Opt {
    /// Commands.
    #[clap(subcommand)]
    pub command: Command,
    /// Enable verbose logs.
    #[clap(short, long)]
    pub verbose: bool,
    /// Gear node rpc endpoint.
    #[clap(short, long)]
    pub endpoint: Option<String>,
    /// Password of the signer account.
    #[clap(short, long)]
    pub passwd: Option<String>,
}

impl Opt {
    /// setup logs
    fn setup_logs(&self) -> Result<()> {
        let mut builder = if self.verbose {
            Builder::from_env(Env::default().default_filter_or("debug"))
        } else {
            match &self.command {
                Command::Claim(_)
                | Command::Create(_)
                | Command::Reply(_)
                | Command::Send(_)
                | Command::Upload(_)
                | Command::UploadProgram(_)
                | Command::Transfer(_) => {
                    let mut builder = Builder::from_env(Env::default().default_filter_or("info"));
                    builder
                        .format_target(false)
                        .format_module_path(false)
                        .format_timestamp(None)
                        .filter_level(LevelFilter::Info);

                    builder
                }
                _ => Builder::from_default_env(),
            }
        };

        builder.try_init()?;
        Ok(())
    }

    /// run program
    pub async fn run() -> Result<()> {
        let opt = Opt::from_args();

        opt.setup_logs()?;
        opt.exec().await?;
        Ok(())
    }

    /// Create api client from endpoint
    async fn api(&self) -> Result<Api> {
        Api::new(self.endpoint.as_deref()).await
    }

    /// Execute command sync
    pub fn exec_sync(&self) -> color_eyre::Result<()> {
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(self.exec()).map_err(Into::into)
    }

    /// Execute command.
    pub async fn exec(&self) -> Result<()> {
        match &self.command {
            Command::Key(key) => key.exec(self.passwd.as_deref())?,
            Command::Login(login) => login.exec()?,
            Command::Meta(meta) => meta.exec()?,
            Command::New(new) => new.exec().await?,
            Command::Program(program) => program.exec(self.api().await?).await?,
            Command::Update(update) => update.exec().await?,
            sub => {
                let signer = Api::new(self.endpoint.as_deref())
                    .await?
                    .try_signer(self.passwd.as_deref())?;

                match sub {
                    Command::Claim(claim) => claim.exec(signer).await?,
                    Command::Create(create) => create.exec(signer).await?,
                    Command::Info(info) => info.exec(signer).await?,
                    Command::Send(send) => send.exec(signer).await?,
                    Command::Upload(upload) => upload.exec(signer).await?,
                    Command::UploadProgram(upload) => upload.exec(signer).await?,
                    Command::Transfer(transfer) => transfer.exec(signer).await?,
                    Command::Reply(reply) => reply.exec(signer).await?,
                    _ => unreachable!("Already matched"),
                }
            }
        }

        Ok(())
    }
}

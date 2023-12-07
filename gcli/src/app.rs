//! Command line application abstraction

use crate::keystore;
use clap::Parser;
use color_eyre::{eyre::eyre, Result};
use env_logger::{Builder, Env};
use gsdk::{signer::Signer, Api};

/// Command line gear program application abstraction.
#[async_trait::async_trait]
pub trait App: Parser {
    /// How many times we'll retry when RPC requests failed.
    fn retry(&self) -> u16 {
        5
    }

    /// The verbosity logging level.
    fn verbose(&self) -> u16 {
        0
    }

    /// The endpoint of the gear node.
    fn endpoint(&self) -> Option<String> {
        None
    }

    /// Password of the signer account.
    fn passwd(&self) -> Option<String> {
        None
    }

    /// Exec program from the parsed arguments.
    async fn exec(self, signer: Signer) -> anyhow::Result<()>;

    /// Run application.
    ///
    /// This is a wrapper of [`Self::exec`] with preset retry
    /// and verbose level.
    async fn run() -> Result<()> {
        color_eyre::install()?;

        let app = Self::parse();
        let name = Self::command().get_name().to_string();
        let filter = match app.verbose() {
            0 => format!("{name}=info"),
            1 => format!("{name}=debug"),
            2 => format!("debug"),
            _ => "trace".into(),
        };

        let mut builder = Builder::from_env(Env::default().default_filter_or(filter));
        builder
            .format_target(false)
            .format_module_path(false)
            .format_timestamp(None);
        builder.try_init()?;

        let signer = {
            let endpoint = app.endpoint().clone();
            let retry = app.retry();
            let passwd = app.passwd();
            let api = Api::new_with_timeout(endpoint.as_deref(), Some(retry.into())).await?;
            let pair = if let Ok(s) = keystore::cache(passwd.as_deref()) {
                s
            } else {
                keystore::keyring(passwd.as_deref())?
            };

            (api, pair).into()
        };

        app.exec(signer)
            .await
            .map_err(|e| eyre!("Failed to run app, {e}"))
    }
}

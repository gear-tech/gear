// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.
//
//! Command line application abstraction

use crate::keyring;
use clap::Parser;
use color_eyre::{Result, eyre::eyre};
use gclient::{
    GearApi,
    ext::sp_core::{self, Pair as _, crypto::Ss58Codec, sr25519::Pair},
};
use gsdk::Api;
use std::env;
use tracing_subscriber::EnvFilter;

/// Command line gear program application abstraction.
///
/// ```ignore
/// use gcli::{async_trait, App, Command, clap::Parser, color_eyre, anyhow};
///
/// /// My customized sub commands.
/// #[derive(Debug, Parser)]
/// pub enum SubCommand {
///     /// GCli preset commands.
///     #[clap(flatten)]
///     GCliCommands(Command),
///     /// My customized ping command.
///     Ping,
/// }
///
/// /// My customized gcli.
/// #[derive(Debug, Parser)]
/// pub struct MyGCli {
///     #[clap(subcommand)]
///     command: SubCommand,
/// }
///
/// #[async_trait]
/// impl App for MyGCli {
///     async fn exec(&self) -> anyhow::Result<()> {
///         match &self.command {
///             SubCommand::GCliCommands(command) => command.exec(self).await,
///             SubCommand::Ping => {
///                 println!("pong");
///                 Ok(())
///             }
///         }
///     }
/// }
///
/// #[tokio::main]
/// async fn main() -> color_eyre::Result<()> {
///     MyGCli::parse().run().await
/// }
/// ```
#[async_trait::async_trait]
pub trait App: Parser + Sync {
    /// Timeout of rpc requests.
    fn timeout(&self) -> u64 {
        60000
    }

    /// The verbosity logging level.
    fn verbose(&self) -> u8 {
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

    /// Get the address of the primary key
    fn ss58_address(&self) -> String {
        keyring::load_keyring()
            .and_then(|mut ring| ring.primary().map(|keystore| keystore.address.clone()))
            .unwrap_or_else(|_| {
                Pair::from_string("//Alice", None)
                    .expect("Alice always works")
                    .public()
                    .to_ss58check()
            })
    }

    /// Exec program from the parsed arguments.
    async fn exec(&self) -> anyhow::Result<()>;

    /// Get gear api without signing in with password.
    async fn api(&self) -> anyhow::Result<GearApi> {
        let endpoint = self.endpoint().clone();
        Api::builder()
            .timeout(self.timeout())
            .build(endpoint.as_deref())
            .await
            .map(Into::into)
    }

    /// Get signer.
    async fn signer(&self) -> anyhow::Result<GearApi> {
        let passwd = self.passwd();

        let api = Api::builder()
            .timeout(self.timeout())
            .build(self.endpoint().as_deref())
            .await?;
        let mut keyring = keyring::load_keyring()?;
        let passphrase = passwd.as_ref().and_then(|p| hex::decode(p).ok());
        let pair = keyring.primary()?.decrypt(passphrase.as_deref())?;

        Ok(GearApi::from((api, pair.into())))
    }

    /// Run application.
    ///
    /// This is a wrapper of [`Self::exec`] with preset retry
    /// and verbose level.
    async fn run(&self) -> Result<()> {
        color_eyre::install()?;
        sp_core::crypto::set_default_ss58_version(runtime_primitives::VARA_SS58_PREFIX.into());

        let name = Self::command().get_name().to_string();
        let filter = if env::var(EnvFilter::DEFAULT_ENV).is_ok() {
            EnvFilter::from_default_env()
        } else {
            match self.verbose() {
                0 => format!("{name}=info,gsdk=info").into(),
                1 => format!("{name}=debug,gsdk=debug").into(),
                2 => "debug".into(),
                _ => "trace".into(),
            }
        };

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .without_time()
            .try_init()
            .map_err(|e| eyre!("{e}"))?;

        self.exec()
            .await
            .map_err(|e| eyre!("Failed to run app, {e}"))
    }
}

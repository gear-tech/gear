// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use crate::keystore;
use clap::Parser;
use color_eyre::{eyre::eyre, Result};
use env_logger::{Builder, Env};
use gsdk::{signer::Signer, Api};

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
    async fn exec(&self) -> anyhow::Result<()>;

    /// Get signer.
    async fn signer(&self) -> anyhow::Result<Signer> {
        let endpoint = self.endpoint().clone();
        let timeout = self.timeout();
        let passwd = self.passwd();

        let api = Api::new_with_timeout(endpoint.as_deref(), Some(timeout)).await?;
        let pair = if let Ok(s) = keystore::cache(passwd.as_deref()) {
            s
        } else {
            keystore::keyring(passwd.as_deref())?
        };

        Ok((api, pair).into())
    }

    /// Run application.
    ///
    /// This is a wrapper of [`Self::exec`] with preset retry
    /// and verbose level.
    async fn run(&self) -> Result<()> {
        color_eyre::install()?;
        sp_core::crypto::set_default_ss58_version(runtime_primitives::VARA_SS58_PREFIX.into());

        let name = Self::command().get_name().to_string();
        let filter = match self.verbose() {
            0 => format!("{name}=info,gsdk=info"),
            1 => format!("{name}=debug,gsdk=debug"),
            2 => "debug".into(),
            _ => "trace".into(),
        };

        let mut builder = Builder::from_env(Env::default().default_filter_or(filter));
        builder
            .format_target(false)
            .format_module_path(false)
            .format_timestamp(None);
        builder.try_init()?;

        self.exec()
            .await
            .map_err(|e| eyre!("Failed to run app, {e}"))
    }
}

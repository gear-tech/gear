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

use crate::{
    cmd::{
        Command,
        config::{ConfigSettings, Endpoint},
    },
    utils::HexBytes,
};
use anyhow::{Context, Result, anyhow};
use clap::Parser;
use gsdk::{Api, SignedApi, ext::sp_core};
use gsigner::schemes::sr25519::{Keyring, Keystore};
use std::{env, fs, io, path::PathBuf, time::Duration};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Parser)]
pub struct Opts {
    /// Timeout for RPC requests, in milliseconds.
    #[arg(short, long, default_value = "60000")]
    pub timeout: u64,

    /// Increase verbosity level, maximum is 3.
    #[clap(short, long = "verbose", action = clap::ArgAction::Count)]
    pub verbosity: u8,

    /// Gear node RPC endpoint.
    ///
    /// Can be `mainnet`, `testnest`, `localhost` or a custom URL.
    #[arg(short, long)]
    pub endpoint: Option<Endpoint>,

    /// Password for the signer account, as hex string.
    #[arg(short, long)]
    pub passwd: Option<HexBytes>,
}

/// Application state.
#[derive(Debug)]
pub struct App {
    opts: Opts,
}

impl App {
    /// Constructs new application instance.
    pub fn new(opts: Opts) -> Self {
        Self { opts }
    }

    pub async fn run(mut self, command: Command) -> Result<()> {
        sp_core::crypto::set_default_ss58_version(runtime_primitives::VARA_SS58_PREFIX.into());

        let name = env!("CARGO_PKG_NAME");
        let filter = if env::var(EnvFilter::DEFAULT_ENV).is_ok() {
            EnvFilter::from_default_env()
        } else {
            match self.opts.verbosity {
                0 => format!("{name}=info,gsdk=info").into(),
                1 => format!("{name}=debug,gsdk=debug").into(),
                2 => "debug".into(),
                _ => "trace".into(),
            }
        };

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .without_time()
            .with_writer(io::stderr)
            .try_init()
            .map_err(|err| anyhow!("{err}"))?;

        command.exec(&mut self).await
    }

    /// Returns the persistent configuration.
    ///
    /// Loads it if it's not loaded yet.
    pub fn config(&self) -> Result<ConfigSettings> {
        ConfigSettings::read()
    }

    /// Returns a Gear node API wrapper.
    pub async fn api(&self) -> Result<Api> {
        let endpoint = match self.opts.endpoint.clone() {
            Some(endpoint) => endpoint,
            None => self.config()?.endpoint,
        };

        Ok(Api::builder()
            .timeout(Duration::from_millis(self.opts.timeout))
            .uri(endpoint.as_str())
            .build()
            .await?)
    }

    /// Returns the keyring.
    pub fn keyring(&self) -> Result<Keyring> {
        let path = env::var_os("GCLI_CONFIG_DIR")
            .map_or_else(store_path, |dir| Ok(PathBuf::from(dir).join("keyring")))?;

        if !path.exists() {
            fs::create_dir_all(&path).context("failed to create keyring directory")?;
        }

        Keyring::load(path)
    }

    /// Returns the currently used keystore.
    pub fn keystore(&self) -> Result<Keystore> {
        let mut keyring = self.keyring()?;
        Ok(keyring.primary()?.clone())
    }

    pub fn ss58_address(&self) -> Result<String> {
        Ok(self.keystore()?.address.to_owned())
    }

    /// Returns a signed Gear node API wrapper.
    pub async fn signed_api(&self) -> Result<SignedApi> {
        let pair = self
            .keystore()?
            .clone()
            .decrypt(self.opts.passwd.as_deref())?;

        Ok(SignedApi::with_pair(self.api().await?, pair.into()))
    }
}

fn store_path() -> Result<PathBuf> {
    let data_dir = dirs::data_dir().ok_or_else(|| anyhow!("Failed to locate app directory."))?;
    let store = gsigner::keyring::resolve_namespaced_path(
        data_dir.join("gsigner"),
        gsigner::keyring::NAMESPACE_SR,
    );
    fs::create_dir_all(&store)?;
    Ok(store)
}

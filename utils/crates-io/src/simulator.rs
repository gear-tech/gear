// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Packages publishing simulator

use crate::{CARGO_REGISTRY_NAME, Workspace};
use anyhow::Result;
use std::{
    fs,
    net::SocketAddr,
    ops::Deref,
    path::{Path, PathBuf},
};
use tempfile::TempDir;
use tokio::task::{self, JoinHandle};
use toml_edit::DocumentMut;

enum RegistryPath {
    Dir(PathBuf),
    TempDir(TempDir),
}

impl Deref for RegistryPath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        match self {
            RegistryPath::Dir(path) => path,
            RegistryPath::TempDir(temp_dir) => temp_dir.path(),
        }
    }
}

/// crates-io packages publishing simulator.
#[allow(dead_code)]
pub struct Simulator {
    path: RegistryPath,
    addr: SocketAddr,
    handle: JoinHandle<()>,
    config_path: PathBuf,
    original_config: DocumentMut,
    mutable_config: DocumentMut,
}

impl Simulator {
    /// Create a new simulator.
    pub fn new(registry_path: Option<PathBuf>) -> Result<Self> {
        let path = match registry_path {
            Some(path) => RegistryPath::Dir(path),
            None => RegistryPath::TempDir(TempDir::new()?),
        };
        let (future, addr) = cargo_http_registry::serve(&path, "127.0.0.1:35503".parse()?)?;
        let handle = task::spawn(future);

        let config_path = Workspace::resolve_path(".cargo/config.toml")?;
        let original_config: DocumentMut = fs::read_to_string(&config_path)?.parse()?;
        let mut mutable_config = original_config.clone();

        // Patch `.cargo/config.toml` according to https://github.com/d-e-s-o/cargo-http-registry/blob/main/README.md#usage
        mutable_config["registry"]["default"] = toml_edit::value(CARGO_REGISTRY_NAME);
        mutable_config["registries"][CARGO_REGISTRY_NAME]["index"] =
            toml_edit::value(format!("http://{addr}/git"));
        mutable_config["registries"][CARGO_REGISTRY_NAME]["token"] = toml_edit::value("token");
        mutable_config["net"]["git-fetch-with-cli"] = toml_edit::value(true);

        Ok(Self {
            path,
            addr,
            handle,
            config_path,
            original_config,
            mutable_config,
        })
    }

    /// Returns socket addr
    pub fn addr(&self) -> &SocketAddr {
        &self.addr
    }

    /// Restore cargo config
    pub fn restore(&self) -> Result<()> {
        fs::write(&self.config_path, self.original_config.to_string()).map_err(Into::into)
    }

    /// Patch cargo config
    pub fn patch(&self) -> Result<()> {
        fs::write(&self.config_path, self.mutable_config.to_string()).map_err(Into::into)
    }
}

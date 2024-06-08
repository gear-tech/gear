// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

//! Gear protocol node wrapper
use crate::{utils, Log};
use anyhow::{anyhow, Result};
use std::{
    env,
    net::SocketAddrV4,
    path::Path,
    process::{Child, Command, Stdio},
};

const GEAR_BINARY: &str = "gear";

/// Gear protocol node wrapper
pub struct Node {
    /// Node logs holder
    log: Log,
    /// Node command
    command: Command,
    /// Node child process
    process: Option<Child>,
    /// Node socket address
    address: Option<SocketAddrV4>,
}

impl Node {
    /// Create a new from gear command that found
    /// in the current system.
    pub fn new() -> Result<Self> {
        Self::from_path(which::which(GEAR_BINARY)?)
    }

    /// Create a new node from path
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            log: Default::default(),
            command: Command::new(path.as_ref()),
            process: None,
            address: None,
        })
    }

    /// Spawn the node
    pub fn spawn(&mut self) -> Result<()> {
        let port: String = utils::pick().to_string();
        let mut process = self
            .command
            .env(
                "RUST_LOG",
                env::var("RUST_LOG").unwrap_or_else(|_| "".into()),
            )
            .args(["--dev", "--no-hardware-benchmarks", "--rpc-port", &port])
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        self.log.spawn(&mut process)?;
        self.address = Some(format!("{}:{port}", utils::LOCALHOST).parse()?);
        Ok(())
    }

    pub fn address(&self) -> Result<SocketAddrV4> {
        self.address.ok_or(anyhow!("Node has not spawned yet"))
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        if let Some(mut ps) = self.process.take() {
            ps.kill().expect("Unable to kill node process.")
        }
    }
}

// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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
use crate::{Log, NodeInstance, utils};
use anyhow::Result;
use std::{
    env,
    path::Path,
    process::{Command, Stdio},
};

const GEAR_BINARY: &str = "gear";
const DEFAULT_ARGS: [&str; 4] = ["--dev", "--tmp", "--no-hardware-benchmarks", "--rpc-port"];

/// Gear protocol node wrapper
pub struct Node {
    /// Node command
    command: Command,
    /// The rpc port of the node if any
    port: Option<u16>,
    /// How many logs should the log holder stores
    logs: Option<usize>,
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
            command: Command::new(path.as_ref()),
            port: None,
            logs: None,
        })
    }

    /// Append argument to the node
    ///
    /// see also [`Node::args`]
    pub fn arg(&mut self, arg: &str) -> &mut Self {
        self.command.arg(arg);
        self
    }

    /// Append arguments to the node
    ///
    /// NOTE: argument `--dev` is managed by [`Node::spawn`] and could not be removed, if
    /// you are about to run a production node, please run the node binary directly.
    pub fn args(&mut self, args: &[&str]) -> &mut Self {
        self.command.args(args);
        self
    }

    /// Sets the rpc port and returns self.
    pub fn rpc_port(&mut self, port: u16) -> &mut Self {
        self.port = Some(port);
        self
    }

    /// The log holder stores 256 lines of matched logs
    /// by default, here in this function we receive a limit
    /// of the logs and resize the logger on spawning.
    pub fn logs(&mut self, limit: usize) -> &mut Self {
        self.logs = Some(limit);
        self
    }

    /// Spawn the node
    pub fn spawn(&mut self) -> Result<NodeInstance> {
        let port = self.port.unwrap_or(utils::pick()).to_string();
        let mut args = DEFAULT_ARGS.to_vec();
        args.push(&port);

        let mut process = self
            .command
            .env(
                "RUST_LOG",
                env::var_os("GEAR_NODE_WRAPPER_LOG")
                    .or_else(|| env::var_os("RUST_LOG"))
                    .unwrap_or_default(),
            )
            .args(args)
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let address = format!("{}:{port}", utils::LOCALHOST).parse()?;
        let mut log = Log::new(self.logs);
        log.spawn(&mut process)?;
        Ok(NodeInstance {
            address,
            log,
            process,
        })
    }
}

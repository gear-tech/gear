// This file is part of Gear.
//
// Copyright (C) 2022-2023 Gear Technologies Inc.
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

use crate::testing::{port, Error, Result};
use std::{
    env,
    ffi::OsStr,
    io::{BufRead, BufReader},
    net::{Ipv4Addr, SocketAddrV4},
    process::{Child, Command, Stdio},
};

/// A struct representing a node running on local PC.
#[derive(Debug)]
pub struct Node {
    process: Child,
    address: SocketAddrV4,
}

impl Node {
    /// Returns the socket address the node is listening to.
    pub fn address(&self) -> SocketAddrV4 {
        self.address
    }

    /// Run node from path with localhost as host.
    pub fn try_from_path(path: impl AsRef<OsStr>, args: Vec<&str>) -> Result<Self> {
        let port = port::pick();
        let port_string = port.to_string();

        let mut args = args;
        args.extend_from_slice(&["--rpc-port", &port_string, "--no-hardware-benchmarks"]);

        let process = Command::new(path)
            .env(
                "RUST_LOG",
                env::var("RUST_LOG").unwrap_or_else(|_| "".into()),
            )
            .args(args)
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let mut node = Self {
            process,
            address: SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), port),
        };

        node.wait_while_initialized()?;

        Ok(node)
    }

    /// Wait until node is initialized.
    pub fn wait_while_initialized(&mut self) -> Result<String> {
        // `#1` here is enough to ensure that node is initialized.
        self.wait_for_log_record("Imported #1 ")
    }

    /// Wait the provided log record is emitted.
    pub fn wait_for_log_record(&mut self, log: &str) -> Result<String> {
        let Some(stderr) = self.process.stderr.as_mut() else {
            return Err(Error::EmptyStderr);
        };

        for line in BufReader::new(stderr).lines().flatten() {
            if line.contains(log) {
                return Ok(line);
            }
        }

        Err(Error::EmptyStderr)
    }

    /// Print node logs
    pub fn print_logs(&mut self) {
        let stderr = self.process.stderr.as_mut();
        let reader = BufReader::new(stderr.expect("Unable to get stderr"));
        for line in reader.lines().flatten() {
            println!("{line}");
        }
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        self.process.kill().expect("Unable to kill node process.")
    }
}

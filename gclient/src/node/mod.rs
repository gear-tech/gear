// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use result::{Error, Result};
use std::{
    ffi::OsStr,
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};
use ws::WSAddress;

mod port;
pub(crate) mod result;
pub(crate) mod ws;

/// A struct representing a node running on local PC.
#[derive(Debug)]
pub(crate) struct Node {
    process: Child,
    ws_address: WSAddress,
}

impl Node {
    /// Returns Web Socket address the node is listening to.
    pub fn ws_address(&self) -> &WSAddress {
        &self.ws_address
    }

    pub fn try_from_path(path: impl AsRef<OsStr>, args: Vec<&str>) -> Result<Self> {
        let port = port::pick();
        let port_string = port.to_string();

        let mut args = args;
        args.push("--ws-port");
        args.push(&port_string);

        let process = Command::new(path)
            .args(args)
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let mut node = Self {
            process,
            ws_address: WSAddress::dev_with_port(port),
        };

        node.wait_while_initialized()?;

        Ok(node)
    }

    fn wait_while_initialized(&mut self) -> Result<String> {
        self.wait_for_log_record("Imported #2 ")
    }

    fn wait_for_log_record(&mut self, log: &str) -> Result<String> {
        let stderr = self.process.stderr.as_mut();
        let reader = BufReader::new(stderr.ok_or(Error::EmptyStderr)?);
        for line in reader.lines().flatten() {
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

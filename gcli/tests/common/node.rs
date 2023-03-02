// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use crate::common::{env, port, Error, Result};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};

/// Run gear with docker.
pub struct Node {
    /// child process
    ps: Child,
    /// websocket port
    port: u16,
}

impl Node {
    /// Node websocket addr.
    pub fn ws(&self) -> String {
        format!("ws://{}:{}", port::LOCALHOST, self.port)
    }

    /// Run gear with docker in development mode.
    pub fn dev() -> Result<Self> {
        let port = port::pick();
        let ps = Command::new(env::bin("gear"))
            .args(["--ws-port", &port.to_string(), "--tmp", "--dev"])
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        Ok(Self { ps, port })
    }

    /// Wait for log string.
    pub fn wait(&mut self, log: &str) -> Result<String> {
        let stderr = self.ps.stderr.as_mut();
        let reader = BufReader::new(stderr.ok_or(Error::EmptyStderr)?);
        for line in reader.lines().flatten() {
            if line.contains(log) {
                return Ok(line);
            }
        }

        Err(Error::EmptyStderr)
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        self.ps.kill().expect("Failed to kill process")
    }
}

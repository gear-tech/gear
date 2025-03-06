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

use crate::Log;
use anyhow::{Result, anyhow};
use std::{net::SocketAddrV4, process::Child};

/// The instance of the node
///
/// NOTE: This instance should be built from [`Node`].
pub struct NodeInstance {
    /// RPC address of this node
    pub address: SocketAddrV4,
    /// Node log interface
    pub(crate) log: Log,
    /// Node process
    pub(crate) process: Child,
}

impl NodeInstance {
    /// Get the RPC address in string.
    ///
    /// NOTE: If you want [`SocketAddrV4`], just call [`NodeInstance::address`]
    pub fn ws(&self) -> String {
        format!("ws://{}", self.address)
    }

    /// Get the recent cached node logs, the max limit is 256 lines.
    pub fn logs(&self) -> Result<Vec<String>> {
        let Ok(logs) = self.log.logs.read() else {
            return Err(anyhow!("Failed to read logs from the node process."));
        };

        Ok(logs.clone().into_vec())
    }
}

impl Drop for NodeInstance {
    fn drop(&mut self) {
        self.process.kill().expect("Unable to kill node process.")
    }
}

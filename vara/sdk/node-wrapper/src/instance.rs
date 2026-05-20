// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::Log;
use anyhow::{Result, anyhow};
use std::{net::SocketAddrV4, process::Child};

/// The instance of the node
///
/// NOTE: This instance should be built from [`Node`](crate::node::Node).
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

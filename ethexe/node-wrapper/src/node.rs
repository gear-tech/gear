// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use crate::instance::InstanceConfig;

use super::instance::VaraEthInstance;
use anyhow::{Context, Result};
use std::{
    env,
    net::{Ipv4Addr, SocketAddrV4},
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

/// Vara.eth default binary name.
const VARA_ETH_BINARY: &str = "ethexe";

/// [VaraEth] default command arguments.
/// Runs dev environment without P2P network.
const DEFAULT_ARGS: &[&str] = &["run", "--dev", "--no-network"];

/// Builder for launching `ethexe` node.
#[derive(Clone, Debug, Default)]
#[must_use = "This Builder struct does nothing unless it is `spawn`ed"]
pub struct VaraEth {
    program: Option<PathBuf>,
    block_time: Option<u32>,
    custom_rpc_port: Option<u16>,
}

impl VaraEth {
    /// Creates an empty Vara.eth builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a Vara.eth builder which will execute `ethexe` at the given path.
    pub fn at<T: Into<PathBuf>>(path: T) -> Self {
        Self::new().path(path)
    }

    /// Sets the `path` for `ethexe` cli.
    ///
    /// By default it's expected that `ethexe` is in `$PATH`.
    pub fn path<T: Into<PathBuf>>(mut self, path: T) -> Self {
        self.program = Some(path.into());
        self
    }

    /// Sets the block-time for anvil node.
    pub fn block_time(mut self, block_time: u32) -> Self {
        self.block_time = Some(block_time);
        self
    }

    pub fn with_custom_rpc(mut self, port: u16) -> Self {
        self.custom_rpc_port = Some(port);
        self
    }

    /// Spawns the [VaraEthInstance] node wrapper.
    fn spawn(self) -> Result<VaraEthInstance> {
        let program_path = match self.program {
            Some(provided_path) => provided_path,
            None => which::which(VARA_ETH_BINARY)
                .with_context(|| "not found {VARA_ETH_BINARY} in $PATH")?,
        };

        let mut command = Command::new(program_path.as_os_str());

        let mut process = command
            .env(
                "RUST_LOG",
                env::var_os("RUST_LOG").unwrap_or("=ethexe=info".into()),
            )
            .args(DEFAULT_ARGS.to_vec())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped());

        // Important: RPC is always enabled, because of DevApi.
        let rpc_port = self.custom_rpc_port.unwrap_or(9944);
        process = process.args(["--rpc-port".into(), rpc_port.to_string()]);
        process = process.args(["--rpc-cors", "all"]);

        // This hack is for killing the `anvil` that internally starts in `ethexe run --dev`.
        #[cfg(unix)]
        {
            process = unsafe {
                process.pre_exec(|| {
                    if libc::setpgid(0, 0) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                })
            };
        }

        let child = process
            .spawn()
            .with_context(|| "failed to spawn Vara.eth node")?;

        let config = InstanceConfig {
            ethereum_rpc: SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8545),
            rpc_addr: SocketAddrV4::new(Ipv4Addr::LOCALHOST, rpc_port),
        };

        Ok(VaraEthInstance { config, child })
    }
}

#[tokio::test]
async fn test_simple_deploy() {
    let veth = VaraEth::at("../../target/debug/ethexe")
        .with_custom_rpc(9955)
        .spawn()
        .unwrap();

    let ws_endpoint = veth.ws_endpoint();
    println!("ws endpoint: {ws_endpoint}");

    let http_endpoint = veth.http_endpoint();
    println!("http endpoint: {http_endpoint}");

    loop {
        match veth.router_address().await {
            Ok(router) => {
                println!("Router address: {router}");
                break;
            }
            Err(err) => {
                eprint!("{err}");
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

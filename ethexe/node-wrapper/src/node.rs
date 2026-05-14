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

use super::{Error, instance::VaraEthInstance};
use jsonrpsee::ws_client::WsClientBuilder;
use std::{
    env,
    ffi::OsString,
    net::{Ipv4Addr, SocketAddrV4},
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Vara.eth default binary name.
const VARA_ETH_BINARY: &str = "ethexe";

/// [VaraEth] default command arguments.
/// Runs dev environment without P2P network.
const DEFAULT_ARGS: &[&str] = &["run", "--dev", "--no-network"];

/// Timeout for waiting for the node starting.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Builder for launching `Vara.eth` node.
///
/// ```no_run
/// use ethexe_node_wrapper::VaraEth;
///
/// async fn do_some_stuff() {
///     let veth = VaraEth::new().spawn_ready().await.unwrap();
///
///     let http_endpoint = veth.http_endpoint();
///     let router = veth.router_address().await.unwrap();
///
///     println!("Vara.eth running at: {http_endpoint}");
///     println!("Router address: {router}");
/// }
/// ```
#[derive(Clone, Debug, Default)]
#[must_use = "This Builder struct does nothing unless it is `spawn`ed"]
pub struct VaraEth {
    program: Option<PathBuf>,
    block_time: Option<u32>,
    custom_rpc_port: Option<u16>,
    timeout: Option<Duration>,
    extra_args: Vec<OsString>,
}

impl VaraEth {
    /// Creates an empty Vara.eth builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a Vara.eth builder which will execute Vara.eth at the given path.
    pub fn at<T: Into<PathBuf>>(path: T) -> Self {
        Self::new().path(path)
    }

    /// Sets the `path` for Vara.eth cli.
    ///
    /// By default it's expected that Vara.eth is in `$PATH`.
    pub fn path<T: Into<PathBuf>>(mut self, path: T) -> Self {
        self.program = Some(path.into());
        self
    }

    /// Sets the timeout which will be used when the Vara.eth instance is launched.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Appends an extra CLI argument passed to Vara.eth.
    pub fn push_arg<T: Into<OsString>>(mut self, arg: T) -> Self {
        self.extra_args.push(arg.into());
        self
    }

    /// Appends extra CLI arguments passed to Vara.eth.
    pub fn push_args<I, T>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        self.extra_args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Sets the block-time for anvil node.
    pub fn block_time(mut self, block_time: u32) -> Self {
        self.block_time = Some(block_time);
        self
    }

    /// Sets the custom RPC port for Vara.eth.
    pub fn with_custom_rpc(mut self, port: u16) -> Self {
        self.custom_rpc_port = Some(port);
        self
    }

    /// Spawns the [VaraEthInstance] node wrapper without waiting for RPC readiness.
    pub fn spawn_immediate(self) -> Result<VaraEthInstance, Error> {
        let program_path = match self.program {
            Some(provided_path) => provided_path,
            None => which::which(VARA_ETH_BINARY).map_err(Error::BinaryNotFound)?,
        };

        let mut command = Command::new(program_path.as_os_str());

        let mut process = command
            .env(
                "RUST_LOG",
                env::var_os("RUST_LOG").unwrap_or("=ethexe=info".into()),
            )
            .args(DEFAULT_ARGS.to_vec())
            .stderr(Stdio::null())
            .stdout(Stdio::null());

        // Important: RPC is always enabled, because of DevApi.
        let rpc_port = self.custom_rpc_port.unwrap_or(9944);
        let rpc_addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, rpc_port);
        process = process.arg("--rpc-port").arg(rpc_port.to_string());

        if let Some(block_time) = self.block_time {
            process = process.arg("--block-time").arg(block_time.to_string());
        }

        if !self.extra_args.is_empty() {
            process = process.args(self.extra_args);
        }

        // This hack is for killing the `anvil` that internally starts in `ethexe run --dev`.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;

            process = unsafe {
                process.pre_exec(|| {
                    if libc::setpgid(0, 0) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                })
            };
        }

        let child = process.spawn().map_err(Error::Spawn)?;

        Ok(VaraEthInstance {
            rpc_addr,
            eth_rpc_addr: SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8545),
            child,
        })
    }

    /// Spawns the [VaraEthInstance] node wrapper and waits until RPC is ready.
    pub async fn spawn_ready(self) -> Result<VaraEthInstance, Error> {
        let timeout = self.timeout.unwrap_or(STARTUP_TIMEOUT);

        let instance = self.spawn_immediate()?;
        wait_for_rpc(instance.ws_endpoint(), timeout).await?;
        Ok(instance)
    }
}

/// Waits for Vara.eth rpc starting.
async fn wait_for_rpc(url: String, timeout: Duration) -> Result<(), Error> {
    let start = Instant::now();

    loop {
        if start + timeout <= Instant::now() {
            return Err(Error::Timeout);
        }

        if WsClientBuilder::new().build(&url).await.is_ok() {
            break Ok(());
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

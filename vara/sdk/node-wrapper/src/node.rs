// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Gear protocol node wrapper
use crate::{Log, NodeInstance, utils};
use anyhow::{Result, anyhow};
use std::{
    env,
    net::{SocketAddrV4, TcpStream},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const GEAR_BINARY: &str = "gear";
const DEFAULT_ARGS: [&str; 4] = ["--dev", "--tmp", "--no-hardware-benchmarks", "--rpc-port"];
const RPC_READY_TIMEOUT: Duration = Duration::from_secs(30);
const RPC_READY_POLL_INTERVAL: Duration = Duration::from_millis(100);
const FIRST_BLOCK_TIMEOUT: Duration = Duration::from_secs(30);
const FIRST_BLOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);

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
        wait_for_rpc(&mut process, &log, address)?;
        wait_for_first_block(&mut process, &log, address)?;
        Ok(NodeInstance {
            address,
            log,
            process,
        })
    }
}

fn wait_for_rpc(process: &mut std::process::Child, log: &Log, address: SocketAddrV4) -> Result<()> {
    let deadline = Instant::now() + RPC_READY_TIMEOUT;

    loop {
        if TcpStream::connect_timeout(&address.into(), RPC_READY_POLL_INTERVAL).is_ok() {
            return Ok(());
        }

        if let Some(status) = process.try_wait()? {
            return Err(anyhow!(
                "node exited before RPC became reachable at {address} with status {status}.\n{}",
                startup_logs(log)
            ));
        }

        if Instant::now() >= deadline {
            let _ = process.kill();
            let _ = process.wait();
            return Err(anyhow!(
                "node RPC at {address} did not become reachable within {:?}.\n{}",
                RPC_READY_TIMEOUT,
                startup_logs(log)
            ));
        }

        thread::sleep(RPC_READY_POLL_INTERVAL);
    }
}

fn wait_for_first_block(
    process: &mut std::process::Child,
    log: &Log,
    address: SocketAddrV4,
) -> Result<()> {
    let deadline = Instant::now() + FIRST_BLOCK_TIMEOUT;

    loop {
        if logs_contain(log, "Imported #1") {
            return Ok(());
        }

        if let Some(status) = process.try_wait()? {
            return Err(anyhow!(
                "node exited before importing first block after RPC became reachable at {address} with status {status}.\n{}",
                startup_logs(log)
            ));
        }

        if Instant::now() >= deadline {
            let _ = process.kill();
            let _ = process.wait();
            return Err(anyhow!(
                "node at {address} did not import first block within {:?} after RPC became reachable.\n{}",
                FIRST_BLOCK_TIMEOUT,
                startup_logs(log)
            ));
        }

        thread::sleep(FIRST_BLOCK_POLL_INTERVAL);
    }
}

fn logs_contain(log: &Log, needle: &str) -> bool {
    log.logs
        .read()
        .is_ok_and(|logs| logs.iter().any(|line| line.contains(needle)))
}

fn startup_logs(log: &Log) -> String {
    match log.logs.read() {
        Ok(logs) if logs.is_empty() => "no startup logs captured".to_string(),
        Ok(logs) => format!("startup logs:\n{}", logs.join("\n")),
        Err(_) => "failed to read startup logs".to_string(),
    }
}

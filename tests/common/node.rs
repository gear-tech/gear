use crate::common::{port, Error, Result};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};

pub const GEAR_NODE_BIN_PATH: &str = "/res/gear-node";

/// Run gear-node with docker.
pub struct Node {
    /// child process
    ps: Child,
    /// websocket port
    port: u16,
}

impl Node {
    /// node websocket addr
    pub fn ws(&self) -> String {
        format!("ws://{}:{}", port::LOCALHOST, self.port)
    }

    /// Run gear-node with docker in development mode.
    pub fn dev() -> Result<Self> {
        let port = port::pick();
        let ps = Command::new(env!("CARGO_MANIFEST_DIR").to_owned() + GEAR_NODE_BIN_PATH)
            .args(&["--ws-port", &port.to_string(), "--tmp", "--dev"])
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        Ok(Self { ps, port })
    }

    /// Wait for the block importing
    pub fn wait(&mut self, log: &str) -> Result<String> {
        let stderr = self.ps.stderr.take();
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

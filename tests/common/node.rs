use crate::common::{docker::Docker, logs::Logs, Error, Result};

pub const GEAR_NODE_BIN_PATH: &str = "/usr/local/bin/gear-node";
pub const GEAR_NODE_DOCKER_IMAGE: &str = "ghcr.io/gear-tech/node:latest";

/// Run gear-node with docker.
pub struct Node(Docker);

impl Node {
    /// Run gear-node with docker in development mode.
    pub fn dev(ws: u16) -> Result<Self> {
        let child = Docker::run(&[
            "-p",
            &format!("{}:9944", ws),
            GEAR_NODE_DOCKER_IMAGE,
            GEAR_NODE_BIN_PATH,
            "--tmp",
            "--dev",
            "--unsafe-ws-external",
        ])?;

        Ok(Self(child))
    }

    /// Spawn logs of gear-node.
    pub fn logs(&mut self) -> Result<Logs> {
        self.0.logs()
    }

    /// Wait for the block importing
    pub fn wait(&mut self, log: &str) -> Result<()> {
        let mut logs: Vec<String> = Default::default();
        for line in self.logs()? {
            if line.contains(log) {
                return Ok(());
            }

            logs.push(line.clone());
        }

        Err(Error::Spawn(logs.join("\n")))
    }
}

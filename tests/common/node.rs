use std::{
    io::{BufRead, BufReader, Error, Lines, Result},
    iter::FilterMap,
    process::{Child, ChildStderr, Command, Stdio},
    result::Result as StdResult,
};

pub const GEAR_NODE_BIN_PATH: &str = "/usr/local/bin/gear-node";
pub const GEAR_NODE_DOCKER_IMAGE: &str = "ghcr.io/gear-tech/node:latest";

/// Run gear-node with docker.
pub struct Node(Child);

impl Node {
    /// Run gear-node with docker in development mode.
    pub fn dev(ws: u16) -> Result<Self> {
        Ok(Command::new("docker")
            .args(&[
                "run",
                "--rm",
                GEAR_NODE_DOCKER_IMAGE,
                GEAR_NODE_BIN_PATH,
                "--tmp",
                "--dev",
                "--ws-port",
                &ws.to_string(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
            .into())
    }

    /// Spawn logs of gear-node.
    pub fn logs(
        &mut self,
    ) -> Option<
        FilterMap<Lines<BufReader<ChildStderr>>, fn(StdResult<String, Error>) -> Option<String>>,
    > {
        Some(
            BufReader::new(self.0.stderr.take()?)
                .lines()
                .filter_map(|line| line.ok()),
        )
    }
}

impl From<Child> for Node {
    fn from(child: Child) -> Self {
        Self(child)
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        self.0.kill().expect("Failed to kill gear-node.")
    }
}

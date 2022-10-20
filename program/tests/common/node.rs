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
    /// node websocket addr
    pub fn ws(&self) -> String {
        format!("ws://{}:{}", port::LOCALHOST, self.port)
    }

    /// Run gear with docker in development mode.
    pub fn dev() -> Result<Self> {
        let port = port::pick();
        let port_string = port.to_string();

        let args = vec!["--ws-port", &port_string, "--tmp", "--dev"];

        #[cfg(all(feature = "vara", not(feature = "gear")))]
        let args = [args, vec!["--force-vara"]].concat();

        let ps = Command::new(env::bin("gear"))
            .args(args)
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

//! docker command
use crate::common::{logs::Logs, traits::Convert, Error, Result};
use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
};

/// Command docker
///
/// Seperating `run` and `logs` since we can not exit the
/// docker container successfully inside one child process.
pub struct Docker(
    /// Docker container id
    String,
);

impl Docker {
    fn cmd() -> Command {
        Command::new("docker")
    }

    /// Run docker containers.
    pub fn run(args: &[&str]) -> Result<Self> {
        Ok(Self(
            Self::cmd()
                .args([&["run", "--rm", "-d"], args].concat())
                .output()?
                .stdout
                .convert()
                .trim()
                .into(),
        ))
    }

    /// Follow the logs from the running container
    pub fn logs(&self) -> Result<Logs> {
        let mut logs = Command::new("docker")
            .args(&["logs", &self.0, "-f"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        Ok(
            BufReader::new(logs.stderr.take().ok_or(Error::EmptyStderr)?)
                .lines()
                .filter_map(|line| line.ok()),
        )
    }

    /// Kill container.
    fn kill(&self) -> Result<()> {
        assert!(Self::cmd()
            .args(&["rm", &self.0, "-f"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .status()?
            .success());

        Ok(())
    }
}

impl Drop for Docker {
    fn drop(&mut self) {
        self.kill()
            .expect(&format!("Failed to remove docker container {}", self.0))
    }
}

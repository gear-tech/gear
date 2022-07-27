//! command gear-node
use std::{
    io::Result,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

/// gear node binary
pub struct Node(
    // Path of gear node binary.
    PathBuf,
);

impl Node {
    /// New gear node command
    pub fn new(p: impl AsRef<Path>) -> Self {
        Self(p.as_ref().into())
    }

    fn cmd(&self) -> Command {
        Command::new(&self.0)
    }

    /// Run dev node
    pub fn dev(&mut self, ws: u16) -> Result<Child> {
        self.cmd()
            .args(["--dev", "--tmp", "--ws-port", &ws.to_string()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    }
}

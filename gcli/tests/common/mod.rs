// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Common utils for integration tests
pub use self::{args::Args, node::NodeExec};

use anyhow::{Context, Result};
use gear_node_wrapper::{Node, NodeInstance};
use std::{
    io::Write,
    process::{Command, Output, Stdio},
};
use tracing_subscriber::EnvFilter;

mod args;
pub mod env;
pub mod node;

impl NodeExec for NodeInstance {
    /// Run binary `gcli`
    fn run(&self, args: Args) -> Result<Output> {
        gcli(args.endpoint(self.ws()))
    }
}

/// Run binary `gcli`
pub fn gcli(args: impl Into<Args>) -> Result<Output> {
    let args = args.into();

    let mut args_vec = Vec::new();
    if let Some(endpoint) = args.endpoint {
        args_vec.extend(["--endpoint".to_string(), endpoint]);
    }
    args_vec.push(args.command);
    args_vec.extend(args.args);
    args_vec.extend(args.with);

    let mut cmd = Command::new(env::gcli_bin())
        .args(args_vec)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // tests rely on filter `gsdk=info`
        .env_remove("RUST_LOG")
        .spawn()
        .context("failed to spawn gcli")?;
    if !args.stdin.is_empty() {
        cmd.stdin
            .as_mut()
            .unwrap()
            .write_all(&args.stdin)
            .context("failed to write stdin")?;
    }
    cmd.wait_with_output().context("failed to run gcli")
}

/// Run the dev node
pub fn dev() -> Result<NodeInstance> {
    login_as_alice()?;
    Node::from_path(env::node_bin())
        .and_then(|mut node| node.spawn())
        .context("failed to spawn node")
}

/// Init env logger
#[allow(dead_code)]
pub fn init_logger() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
}

/// Login as //Alice
pub fn login_as_alice() -> Result<()> {
    let _ = gcli(["wallet", "dev"])?;

    Ok(())
}

/// Create program messenger
pub async fn create_messenger() -> Result<NodeInstance> {
    let node = dev()?;

    let args = Args::new("upload").program_stdin(demo_messenger::WASM_BINARY);
    let output = node.run(args)?;

    assert!(
        output.status.success(),
        "failed with stderr:\n\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(node)
}

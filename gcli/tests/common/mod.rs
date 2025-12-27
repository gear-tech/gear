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
use anyhow::{Context, Result, bail};
use gear_node_wrapper::{Node, NodeInstance};
use std::{
    ffi::OsStr,
    process::{Output, Stdio},
};
use tokio::{
    io::AsyncWriteExt,
    process::{Child, Command},
};

pub mod env;

pub trait NodeExec {
    async fn gcli(&self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Result<Output>;
    async fn gcli_with_stdin(
        &self,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
        buf: &[u8],
    ) -> Result<Output>;
}

async fn spawn_gcli(
    node: &NodeInstance,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<Child> {
    Ok(Command::new(env::gcli_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("RUST_LOG")
        .args(["--endpoint", &node.ws()])
        .args(args)
        .spawn()?)
}

async fn wait_gcli(child: Child) -> Result<Output> {
    let output = child.wait_with_output().await?;

    if !output.status.success() {
        bail!(
            "process `gcli` exited with non-zero code, stderr:\n\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    }

    Ok(output)
}

impl NodeExec for NodeInstance {
    async fn gcli(&self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Result<Output> {
        wait_gcli(spawn_gcli(self, args).await?).await
    }

    async fn gcli_with_stdin(
        &self,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
        buf: &[u8],
    ) -> Result<Output> {
        let mut child = spawn_gcli(self, args).await?;
        let mut stdin = child
            .stdin
            .take()
            .context("failed to get child process stdin")?;
        stdin.write_all(buf).await?;
        drop(stdin);

        wait_gcli(child).await
    }
}

/// Run the dev node
pub async fn dev() -> Result<NodeInstance> {
    login_as_alice().await?;
    Node::from_path(env::node_bin())
        .and_then(|mut node| node.spawn())
        .context("failed to spawn node")
}

/// Login as //Alice
pub async fn login_as_alice() -> Result<()> {
    let output = Command::new(env::gcli_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("RUST_LOG")
        .args(["wallet", "dev"])
        .output()
        .await?;

    assert!(
        output.status.success(),
        "Command failed with stderr:\n\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}

/// Create program messenger
pub async fn create_messenger() -> Result<NodeInstance> {
    let node = dev().await?;

    let output = node
        .gcli_with_stdin(["deploy", "--stdin"], demo_messenger::WASM_BINARY)
        .await?;

    assert!(
        output.status.success(),
        "failed with stderr:\n\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(node)
}

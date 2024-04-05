// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use anyhow::{anyhow, Result};
use cargo_gbuild::GBuild;
use gclient::{EventProcessor, GearApi};
use std::path::PathBuf;

fn node() -> PathBuf {
    let node = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
    node.join(if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    })
    .join("gear")
}

#[tokio::test]
async fn compile_program() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("program/Cargo.toml");
    let artifact = GBuild {
        manifest_path: root.to_string_lossy().to_string().into(),
        features: vec!["debug".into()],
        profile: None,
        target_dir: None,
        release: false,
        meta: false,
    }
    .run()?;

    // Set up testing environment.
    let api = GearApi::dev_from_path(&node())
        .await
        .map_err(|e| anyhow!("{e}, node path: {node:?}"))?;

    // Upload program to the node.
    let mut listener = api.subscribe().await?;
    let (init_mid, pid, _hash) = api
        .upload_program_by_path(artifact.program, b"", b"PING", 2_000_000_000, 0)
        .await?;

    // 1. verify the reply from the init logic.
    let (_mid, payload, _value) = listener.reply_bytes_on(init_mid).await?;
    assert_eq!(payload.map_err(|e| anyhow!(e))?, b"PONG");

    // 2. verify the reply from the handle logic.
    let (handle_mid, _hash) = api.send_message(pid, b"PING", 2_000_000_000, 0).await?;
    let (_mid, payload, _value) = listener.reply_bytes_on(handle_mid).await?;
    assert_eq!(payload.map_err(|e| anyhow!(e))?, b"PONG");

    Ok(())
}

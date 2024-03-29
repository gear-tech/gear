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

use anyhow::Result;
use cargo_gbuild::GBuild;
use gclient::{GearApi, WSAddress};
use std::path::PathBuf;

#[tokio::test]
async fn compile_program() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("program/Cargo.toml");
    let artifact = GBuild {
        manifest_path: root.to_string_lossy().to_string().into(),
        features: vec![],
        target_dir: None,
        release: false,
        meta: false,
    }
    .build()?;

    // Upload the program to the chain
    // let node = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/release/gear");
    // let api = GearApi::dev_from_path(node).await?;
    let api = GearApi::init(WSAddress::dev()).await?;
    api.upload_program_by_path(artifact.program, b"", b"PING", 200_000_000, 0)
        .await?;

    Ok(())
}

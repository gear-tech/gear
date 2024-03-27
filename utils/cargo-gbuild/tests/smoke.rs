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
use std::path::PathBuf;

#[test]
fn compile_program() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("program/Cargo.toml");

    GBuild {
        manifest_path: root.to_string_lossy().to_string().into(),
        features: vec![],
        target_dir: None,
        no_wasm_opt: true,
        meta: false,
    }
    .build()?;

    Ok(())
}

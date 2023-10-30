// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use common::env;
use gcli::VARA_SS58_PREFIX;
use gsdk::{result::Error, Api};
use std::{fs, path::PathBuf};

mod cmd;
mod common;

#[tokio::test]
async fn api_timeout() {
    assert!(matches!(
        Api::new_with_timeout(None, Some(1)).await.err(),
        Some(Error::SubxtRpc(jsonrpsee::core::Error::Transport(..)))
    ));
}

#[test]
fn paths() {
    [
        env::bin("gear"),
        env::bin("gcli"),
        env::wasm_bin("demo_new_meta.opt.wasm"),
        env::wasm_bin("demo_new_meta.meta.txt"),
    ]
    .into_iter()
    .for_each(|path| {
        if !PathBuf::from(&path).exists() {
            panic!("{} not found.", path)
        }
    })
}

#[test]
fn ss58_prefix() -> anyhow::Result<()> {
    let vara = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../runtime/vara/src/lib.rs"),
    )?;

    assert!(vara.contains(&format!("pub const SS58Prefix: u8 = {VARA_SS58_PREFIX};")));
    Ok(())
}

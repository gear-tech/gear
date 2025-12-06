// This file is part of Gear.
//
// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use std::{env, path::PathBuf};

use gsdk::gear;
use utils::dev_node;

mod utils;

fn runtime_wasm() -> PathBuf {
    PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../target/release/wbuild/vara-runtime/vara_runtime.compact.compressed.wasm")
}

#[tokio::test]
async fn set_code_succeed() {
    let (_node, api) = dev_node().await;

    api.set_code_without_checks_by_path(runtime_wasm())
        .await
        .unwrap();
}

#[tokio::test]
async fn set_code_failed() {
    let (_node, api) = dev_node().await;

    let err = api.set_code_by_path(runtime_wasm()).await.unwrap_err();

    assert!(
        matches!(
            err,
            gsdk::Error::Runtime(gear::Error::System(
                gear::system::Error::SpecVersionNeedsToIncrease
            ))
        ),
        "{err:?}"
    );
}

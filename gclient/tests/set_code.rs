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

use gclient::{
    GearApi,
    errors::{self, ModuleError},
};

const RUNTIME_WASM: &str =
    "../target/release/wbuild/vara-runtime/vara_runtime.compact.compressed.wasm";

#[tokio::test]
async fn set_code_succeed() {
    let api = GearApi::dev_from_path("../target/release/gear")
        .await
        .unwrap();
    let _block_hash = api
        .set_code_without_checks_by_path(RUNTIME_WASM)
        .await
        .unwrap();
}

#[tokio::test]
async fn set_code_failed() {
    let api = GearApi::dev_from_path("../target/release/gear")
        .await
        .unwrap();
    let err = api.set_code_by_path(RUNTIME_WASM).await.unwrap_err();

    assert!(
        matches!(
            err,
            gclient::Error::Module(ModuleError::System(
                errors::System::SpecVersionNeedsToIncrease
            ))
        ),
        "{err:?}"
    );
}

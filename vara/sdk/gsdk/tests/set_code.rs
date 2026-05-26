// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gsdk::gear;
use std::{env, path::PathBuf};
use utils::dev_node;

mod utils;

fn runtime_wasm() -> PathBuf {
    PathBuf::from(env::var_os("GEAR_WORKSPACE_DIR").unwrap())
        .join("target/release/wbuild/vara-runtime/vara_runtime.compact.compressed.wasm")
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

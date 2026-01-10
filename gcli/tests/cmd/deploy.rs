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

//! Integration tests for command `deploy`.

use crate::common::{NodeExec, dev};
use anyhow::Result;
use demo_fungible_token::InitConfig;
use gear_core::ids::{CodeId, prelude::CodeIdExt};
use gsdk::Api;
use scale_info::scale::Encode;

#[tokio::test]
async fn test_command_deploy_works() -> Result<()> {
    let node = dev().await?;
    let api = Api::new(node.ws().as_str()).await?.signed_as_alice();
    let code_id = CodeId::generate(demo_fungible_token::WASM_BINARY);
    assert!(
        api.instrumented_code_storage(code_id).await.is_err(),
        "code should not exist"
    );

    let payload = hex::encode(InitConfig::test_sequence().encode());

    let output = node
        .gcli_with_stdin(
            ["deploy", "--init-payload", &payload, "--stdin"],
            demo_fungible_token::WASM_BINARY,
        )
        .await?;

    assert!(
        String::from_utf8(output.stdout)?.contains("Successfully deployed the program"),
        "code should be uploaded, but got: '{}'",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        api.instrumented_code_storage(code_id).await.is_ok(),
        "code should exist"
    );

    Ok(())
}

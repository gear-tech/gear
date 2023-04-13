// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Integration tests for command `upload`
use crate::common::{self, env, logs, traits::Convert, Args, Result};
use gear_core::ids::CodeId;
use gsdk::Api;

#[tokio::test]
async fn test_command_upload_works() {
    common::login_as_alice().expect("login failed");
    let mut node = common::Node::dev().expect("failed to start node");
    node.wait(logs::gear_node::IMPORTING_BLOCKS)
        .expect("node timeout");

    let signer = Api::new(Some(&node.ws()))
        .await
        .expect("build api failed")
        .signer("//Alice", None)
        .expect("get signer failed");

    let code_id = CodeId::generate(demo_new_meta::WASM_BINARY);
    assert!(signer.api().code_storage(code_id).await.is_err());

    let output = node
        .run(Args::new("upload").program(env::wasm_bin("demo_new_meta.opt.wasm")))
        .expect("run command upload failed");

    assert!(output
        .stderr
        .convert()
        .contains(logs::gear_program::EX_UPLOAD_PROGRAM));
    assert!(signer.api().code_storage(code_id).await.is_ok());
}

#[tokio::test]
async fn test_command_upload_program_works() -> Result<()> {
    common::login_as_alice().expect("login failed");
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let output = node.run(
        Args::new("upload")
            .flag("--code-only")
            .program(env::wasm_bin("demo_meta.opt.wasm")),
    )?;

    assert!(output
        .stderr
        .convert()
        .contains(logs::gear_program::EX_UPLOAD_CODE));
    Ok(())
}

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

//! Integration tests for command `deploy`
use crate::common::{self, env, logs, traits::Convert, Args, Result};

#[tokio::test]
async fn test_command_upload_program_works() -> Result<()> {
    common::login_as_alice().expect("login failed");
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let output =
        node.run(Args::new("upload-program").program(env::wasm_bin("demo_meta.opt.wasm")))?;
    let stderr = output.stderr.convert();
    if !stderr.contains(logs::gear_program::EX_UPLOAD_PROGRAM) {
        panic!("{stderr}")
    }

    Ok(())
}

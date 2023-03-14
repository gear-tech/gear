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

//! Integration tests for command `program`
use crate::common::{self, env, logs, traits::Convert, Result};
use demo_meta::{Id, MessageInitIn, Person, Wallet};
use parity_scale_codec::Encode;

// ( issue #2367 )
#[ignore]
#[tokio::test]
async fn test_command_state_works() -> Result<()> {
    common::login_as_alice().expect("login failed");

    // setup node
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    // get demo meta
    let opt = env::wasm_bin("demo_meta.opt.wasm");
    let meta = env::wasm_bin("demo_meta.meta.wasm");

    // Deploy demo_meta
    let deploy = common::gear(&[
        "-e",
        &node.ws(),
        "upload-program",
        &opt,
        "",
        &hex::encode(
            MessageInitIn {
                amount: 42,
                currency: "GEAR".into(),
            }
            .encode(),
        ),
        "20000000000",
    ])?;

    assert!(deploy
        .stderr
        .convert()
        .contains(logs::gear_program::EX_UPLOAD_PROGRAM));

    // Get program id
    let pid = common::program_id(demo_meta::WASM_BINARY, &[]);

    // Query state of demo_meta
    let state = common::gear(&[
        "-e",
        &node.ws(),
        "program",
        &hex::encode(pid),
        "state",
        &meta,
        "--msg",
        "0x00", // None
    ])?;

    let default_wallets = vec![
        Wallet {
            id: Id {
                decimal: 1,
                hex: vec![1u8],
            },
            person: Person {
                surname: "SomeSurname".into(),
                name: "SomeName".into(),
            },
        },
        Wallet {
            id: Id {
                decimal: 2,
                hex: vec![2u8],
            },
            person: Person {
                surname: "OtherName".into(),
                name: "OtherSurname".into(),
            },
        },
    ];

    assert!(state
        .stdout
        .convert()
        .contains(&hex::encode(default_wallets.encode())));

    Ok(())
}

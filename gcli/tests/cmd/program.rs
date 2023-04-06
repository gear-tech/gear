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
use crate::common::{self, env, logs, traits::Convert, Args, Result};
use demo_new_meta::{MessageInitIn, Wallet};
use scale_info::scale::Encode;

#[tokio::test]
async fn test_command_program_state_works() -> Result<()> {
    common::login_as_alice().expect("login failed");

    // Setup node.
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    // Deploy demo_new_meta.
    let opt = env::wasm_bin("demo_new_meta.opt.wasm");
    let _ = node.run(
        Args::new("upload").program(opt).payload(hex::encode(
            MessageInitIn {
                amount: 42,
                currency: "GEAR".into(),
            }
            .encode(),
        )),
    )?;

    // Query state of demo_meta
    let pid = common::program_id(demo_new_meta::WASM_BINARY, &[]);
    let state = node.run(Args::new("program").action("state").program(pid))?;

    // Verify result
    assert_eq!(
        state.stdout.convert().trim_start_matches("0x").trim(),
        hex::encode(Wallet::test_sequence().encode())
    );
    Ok(())
}

const DEMO_NEW_META_METADATA: &str = r#"
Metadata {
    init:  {
        input: MessageInitIn {
            amount: u8,
            currency: String,
        },
        output: MessageInitOut {
            exchange_rate: Result<u8, u8>,
            sum: u8,
        },
    },
    handle:  {
        input: MessageIn {
            id: Id,
        },
        output: MessageOut {
            res: Option<Wallet>,
        },
    },
    reply:  {
        input: str,
        output: [u16],
    },
    others:  {
        input: MessageAsyncIn {
            empty: (),
        },
        output: Option<u8>,
    },
    signal: (),
    state: [Wallet { id: Id, person: Person }],
}
"#;

#[test]
fn test_command_program_metadata_works() -> Result<()> {
    let meta = env::example_path("new-meta/demo_new_meta.meta.txt");
    let args = Args::new("program").action("meta").meta(meta);
    let result = common::gcli(Vec::<String>::from(args)).expect("run gcli failed");

    assert_eq!(
        result.stdout.convert().trim(),
        DEMO_NEW_META_METADATA.trim()
    );
    Ok(())
}

#[test]
fn test_command_program_metadata_derive_works() -> Result<()> {
    let meta = env::example_path("new-meta/demo_new_meta.meta.txt");
    let args = Args::new("program")
        .action("meta")
        .meta(meta)
        .flag("--derive")
        .derive("Person");

    let result = common::gcli(Vec::<String>::from(args)).expect("run gcli failed");
    assert_eq!(
        result.stdout.convert().trim(),
        "Person { surname: String, name: String }"
    );
    Ok(())
}

const META_WASM_V1_OUTPUT: &str = r#"
Exports {
    first_and_last_wallets:  {
        input: (),
        output: (
            Option<Wallet>,
            Option<Wallet>,
        ),
    },
    first_wallet:  {
        input: (),
        output: Option<Wallet>,
    },
    last_wallet:  {
        input: (),
        output: Option<Wallet>,
    },
}
"#;

#[test]
fn test_command_program_metawasm_works() -> Result<()> {
    let meta = env::wasm_bin("demo_meta_state_v1.meta.wasm");
    let args = Args::new("program").action("meta").meta(meta);
    let result = common::gcli(Vec::<String>::from(args)).expect("run gcli failed");

    assert_eq!(result.stdout.convert().trim(), META_WASM_V1_OUTPUT.trim());
    Ok(())
}

#[test]
fn test_command_program_metawasm_derive_works() -> Result<()> {
    let meta = env::wasm_bin("demo_meta_state_v1.meta.wasm");
    let args = Args::new("program")
        .action("meta")
        .meta(meta)
        .flag("--derive")
        .derive("Person");

    let result = common::gcli(Vec::<String>::from(args)).expect("run gcli failed");
    assert_eq!(
        result.stdout.convert().trim(),
        "Person { surname: String, name: String }"
    );
    Ok(())
}

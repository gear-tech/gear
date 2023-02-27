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

//! Integration tests for command `meta`
use crate::common::{self, env, Result};

const DEMO_METADATA: &str = r#"
Metadata {
    meta_title: Example program with metadata,
    meta_init_input: MessageInitIn {
        amount: u8,
        currency: String,
    },
    meta_init_output: MessageInitOut {
        exchange_rate: Result<u8, u8>,
        sum: u8,
    },
    meta_async_init_input: MessageInitAsyncIn {
        empty: (),
    },
    meta_async_init_output: MessageInitAsyncOut {
        empty: (),
    },
    meta_handle_input: MessageIn {
        id: Id,
    },
    meta_handle_output: MessageOut {
        res: Option<Wallet>,
    },
    meta_async_handle_input: MessageHandleAsyncIn {
        empty: (),
    },
    meta_async_handle_output: MessageHandleAsyncOut {
        empty: (),
    },
    meta_state_input: Option<Id>,
    meta_state_output: Vec<Wallet>,
}
"#;

#[tokio::test]
async fn test_display_metadata_works() -> Result<()> {
    let output = common::gear(&["meta", &env::wasm_bin("demo_meta.meta.wasm"), "display"])?;

    assert_eq!(
        DEMO_METADATA.trim(),
        String::from_utf8_lossy(&output.stdout).trim()
    );

    Ok(())
}

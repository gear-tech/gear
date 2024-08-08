// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use anyhow::Result;
use demo_proxy::{InputArgs, WASM_BINARY};
use gclient::{Backend, Client, Message};

async fn test_ping<T: Backend>(client: Client<T>) -> Result<()> {
    let prog = client
        .deploy(
            WASM_BINARY,
            InputArgs {
                destination: [0; 32],
            },
        )
        .await?
        .result;

    let ping = b"ping";
    let result = prog.send(Message::bytes(ping)).await?;

    assert!(
        result
            .logs
            .clone()
            .into_iter()
            .any(|log| { log.payload_bytes() == ping }),
        "Could not find sent message: {result:#?}"
    );

    Ok(())
}

#[tokio::test]
async fn test_gtest() -> Result<()> {
    test_ping(Client::gtest()).await
}

#[tokio::test]
async fn test_gclient() -> Result<()> {
    test_ping(Client::gclient("../target/release/gear").await?).await
}

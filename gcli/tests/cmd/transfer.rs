// This file is part of Gear.
//
// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use crate::common::{self, Args, NodeExec, Result};
use gsdk::Api;

// Testing account
const SURI: &str = "tumble tenant update heavy sad draw present tray atom chunk animal exhaust";
const ADDRESS: &str = "kGhmTEymraqSPa1NYjXzqbko2p4Ge1CmEfACtC1s4aC5hTPYk";

#[tokio::test]
async fn test_command_transfer_works() -> Result<()> {
    let node = common::dev()?;

    // Get balance of the testing address
    let signer = Api::new(node.ws()).await?.signer(SURI, None)?;
    let before = signer.api().get_balance(ADDRESS).await.unwrap_or(0);

    // Run command transfer
    let value = 1_000_000_000_000_000u128;
    let _ = node.run(
        Args::new("transfer")
            .destination(ADDRESS)
            .amount(value.to_string()),
    )?;

    let after = signer.api().get_balance(ADDRESS).await.unwrap_or(0);
    assert_eq!(
        after.saturating_sub(before),
        value,
        "Alice should have received {value}. Balance must be {correct_balance}, but now it is {after}",
        correct_balance = before.saturating_add(value)
    );

    Ok(())
}

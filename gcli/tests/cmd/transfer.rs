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

//! Integration tests for command `deploy`

use crate::common::{self, Args, NodeExec};
use anyhow::Result;
use gsdk::{
    Api,
    ext::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32},
};

// Testing account
const SURI: &str = "tumble tenant update heavy sad draw present tray atom chunk animal exhaust";
const ADDRESS: &str = "kGhmTEymraqSPa1NYjXzqbko2p4Ge1CmEfACtC1s4aC5hTPYk";

#[tokio::test]
async fn test_command_transfer_works() -> Result<()> {
    let node = common::dev()?;

    // Get balance of the testing address
    let signer = Api::new(node.ws().as_str()).await?.signed(SURI, None)?;
    let address = AccountId32::from_ss58check(ADDRESS).map_err(gsdk::Error::from)?;

    let before = signer.unsigned().free_balance(&address).await.unwrap_or(0);

    // Run command transfer
    let value = 1_000_000_000_000_000u128;
    let _ = node.run(
        Args::new("transfer")
            .destination(ADDRESS)
            .amount(value.to_string()),
    )?;

    let after = signer.unsigned().free_balance(&address).await.unwrap_or(0);
    assert_eq!(
        after.saturating_sub(before),
        value,
        "Alice should have received {value}. Balance must be {correct_balance}, but now it is {after}",
        correct_balance = before.saturating_add(value)
    );

    Ok(())
}

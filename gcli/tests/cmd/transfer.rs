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
#![cfg(not(feature = "vara-testing"))]
use crate::common::{self, logs, Args, Result};
use gsdk::Api;

// Testing account
//
// Secret phrase:     tumble tenant update heavy sad draw present tray atom chunk animal exhaust
// Network ID:        substrate
// Secret seed:       0xd13d64420f7e304a1bfd4a17a5cda3f14b4e98034abe2cbd4fc05214c6ba2488
// Public key (hex):  0x62bd03f963e636deea9139b00e33e6800f3c1afebb5f69b47ed07c07be549e78
// Account ID:        0x62bd03f963e636deea9139b00e33e6800f3c1afebb5f69b47ed07c07be549e78
// Public key (SS58): 5EJAhWN49JDfn58DpkERvCrtJ5X3sHue93a1hH4nB9KngGSs
// SS58 Address:      5EJAhWN49JDfn58DpkERvCrtJ5X3sHue93a1hH4nB9KngGSs
const SURI: &str = "tumble tenant update heavy sad draw present tray atom chunk animal exhaust";
const ADDRESS: &str = "5EJAhWN49JDfn58DpkERvCrtJ5X3sHue93a1hH4nB9KngGSs";

#[tokio::test]
async fn test_command_transfer_works() -> Result<()> {
    common::login_as_alice()?;
    let mut node = common::Node::dev()?;

    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    // Get balance of the testing address
    let signer = Api::new(Some(&node.ws())).await?.signer(SURI, None)?;
    let before = signer.api().get_balance(ADDRESS).await.unwrap_or(0);

    // Run command transfer
    let value = 1_000_000_000u128;
    let _ = node.run(
        Args::new("transfer")
            .destination(ADDRESS)
            .amount(value.to_string()),
    )?;

    let after = signer.api().get_balance(ADDRESS).await?;
    assert_eq!(after.saturating_sub(before), value);

    Ok(())
}

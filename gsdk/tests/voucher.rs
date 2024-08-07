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

use gsdk::{
    metadata::runtime_types::pallet_gear_voucher::internal::VoucherId, Api, Event, Result,
    TxInBlock,
};
use utils::{alice_account_id, dev_node};

mod utils;

#[tokio::test]
async fn test_issue_voucher() -> Result<()> {
    let node = dev_node();

    let signer = Api::new(node.ws().as_str())
        .await?
        .signer("//Alice", None)?;
    let account_id = alice_account_id();

    let voucher_initial_balance = 0;

    let tx = signer
        .calls
        .issue_voucher(account_id, voucher_initial_balance, None, false, 100)
        .await?;

    let _voucher_id = get_last_voucher_id(tx).await?;

    Ok(())
}

async fn get_last_voucher_id(tx: TxInBlock) -> Result<VoucherId> {
    for event in tx.wait_for_success().await?.iter() {
        if let Event::GearVoucher(
            gsdk::metadata::runtime_types::pallet_gear_voucher::pallet::Event::VoucherIssued {
                voucher_id,
                ..
            },
        ) = event?.as_root_event::<Event>()?
        {
            dbg!(&voucher_id);
            return Ok(voucher_id);
        }
    }
    panic!("voucher not issued");
}

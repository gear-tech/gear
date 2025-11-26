// This file is part of Gear.
//
// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use gsdk::{Api, Result, backtrace::BacktraceStatus, signer::Signer};
use utils::{alice_account_id, dev_node};

mod utils;

#[tokio::test]
async fn transfer_backtrace() -> Result<()> {
    let node = dev_node();
    let api = Api::new(node.ws().as_str()).await?;
    let signer = Signer::new(api, "//Alice", None)?;
    let alice: [u8; 32] = *alice_account_id().as_ref();

    let tx = signer.calls().transfer_keep_alive(alice, 42).await?;
    let backtrace = signer
        .backtrace()
        .get(tx.extrinsic_hash())
        .expect("Failed to get backtrace of transfer");

    assert!(
        matches!(
            backtrace.values().collect::<Vec<_>>()[..],
            [
                BacktraceStatus::InBestBlock { .. },
                BacktraceStatus::InFinalizedBlock { .. },
            ]
        ),
        "Event backtrace mismatched"
    );
    Ok(())
}

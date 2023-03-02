// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use codec::Encode;
use frame_support::traits::{OnFinalize, OnInitialize};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_common::QueueRunner;
use gear_runtime::{Authorship, BlockGasLimit, Gear, GearGas, GearMessenger, Runtime, System};
use pallet_gear::BlockGasLimitOf;
use sp_consensus_babe::{
    digests::{PreDigest, SecondaryPlainPreDigest},
    BABE_ENGINE_ID,
};
use sp_consensus_slots::Slot;
use sp_runtime::{Digest, DigestItem, Perbill};

/// This is not set to `BlockGasLimitOf::<Runtime>::get`, because of the
/// known possible dead-lock for the message in the queue, when it's valid gas
/// limit is more than maximum possible gas rest for the queue execution.
// # TODO 2328
pub fn default_gas_limit() -> u64 {
    Perbill::from_percent(95).mul_ceil(BlockGasLimit::get())
}

/// Run gear-protocol to the next block with max gas given for the execution.
pub fn run_to_next_block() {
    run_to_block(System::block_number() + 1, None);
}

/// Run gear-protocol until the block `n` giving `remaining_weight` for each block.
pub fn run_to_block(n: u32, remaining_weight: Option<u64>) {
    while System::block_number() < n {
        System::on_finalize(System::block_number());
        initialize(System::block_number() + 1);
        on_initialize();
        let remaining_weight = remaining_weight.unwrap_or_else(BlockGasLimitOf::<Runtime>::get);
        Gear::run_queue(remaining_weight);
        on_finalize_without_system();
    }
}

/// Initialize a new block.
pub fn initialize(new_bn: BlockNumberFor<Runtime>) {
    log::debug!("📦 Initializing block {}", new_bn);

    // All blocks are to be authored by validator at index 0
    let slot = Slot::from(0);
    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(
            BABE_ENGINE_ID,
            PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
                slot,
                authority_index: 0,
            })
            .encode(),
        )],
    };

    System::initialize(&new_bn, &System::parent_hash(), &pre_digest);
    System::set_block_number(new_bn);
}

/// Run `on_initialize hooks` in order as they appear in `AllPalletsWithSystem`.
pub fn on_initialize() {
    System::on_initialize(System::block_number());
    Authorship::on_initialize(System::block_number());
    GearGas::on_initialize(System::block_number());
    GearMessenger::on_initialize(System::block_number());
    Gear::on_initialize(System::block_number());
}

/// Run on_finalize hooks in pallets reversed order, as they appear in `AllPalletsWithSystem`.
// TODO #2307
pub fn on_finalize_without_system() {
    let bn = System::block_number();
    Gear::on_finalize(bn);
    GearMessenger::on_finalize(bn);
    GearGas::on_finalize(bn);
    Authorship::on_finalize(bn);
}

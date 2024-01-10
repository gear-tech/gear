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

//! Mock runtime for gear bridges pallet.

use crate as pallet_gear_bridges;
use frame_support::{construct_runtime, parameter_types};
use frame_system::{self as system};
use primitive_types::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup, Keccak256},
    BuildStorage,
};

type Block = frame_system::mocking::MockBlock<Test>;
pub type AccountId = u64;
pub type BlockNumber = u64;
type Balance = u128;

parameter_types! {
    pub const MaxQueueLength: u32 = 16;
    pub const BlockHashCount: BlockNumber = 250;
    pub const ExistentialDeposit: Balance = 100_000;
}

impl pallet_gear_bridges::Config for Test {
    type MaxQueueLength = MaxQueueLength;
    type Hasher = Keccak256;
    type HashOut = H256;
}

construct_runtime!(
    pub enum Test
    {
        System: system,
        Balances: pallet_balances,
        GearBridges: pallet_gear_bridges
    }
);

common::impl_pallet_system!(Test, DbWeight = (), BlockWeights = ());
common::impl_pallet_balances!(Test);

pub fn new_test_ext() -> sp_io::TestExternalities {
    let t = <frame_system::GenesisConfig<Test> as BuildStorage>::build_storage(
        &system::GenesisConfig::default(),
    )
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

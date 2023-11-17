// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

//! Module contains macros that help to implement Config type
//! for various pallets of Substrate.
//! All used types should be in scope.

use frame_support::{pallet_prelude::*, weights::RuntimeDbWeight};
use frame_system::limits::BlockWeights;
use sp_arithmetic::Perbill;

#[macro_export]
macro_rules! impl_pallet_balances {
    ($runtime:ty) => {
        impl pallet_balances::Config for $runtime {
            type MaxLocks = ();
            type MaxReserves = ();
            type ReserveIdentifier = [u8; 8];
            type Balance = Balance;
            type DustRemoval = ();
            type RuntimeEvent = RuntimeEvent;
            type ExistentialDeposit = ExistentialDeposit;
            type AccountStore = System;
            type WeightInfo = ();
        }
    };
}

pub const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
pub const MAX_BLOCK: u64 = 250_000_000_000;

frame_support::parameter_types! {
    pub RuntimeBlockWeights: BlockWeights = BlockWeights::with_sensible_defaults(
        Weight::from_parts(MAX_BLOCK, u64::MAX),
        NORMAL_DISPATCH_RATIO,
    );
    pub const SS58Prefix: u8 = 42;
    pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight { read: 1_110, write: 2_300 };
    pub const MinimumPeriod: u64 = 500;
}

#[macro_export]
macro_rules! impl_pallet_system {
    ($( $tokens:tt )*) => {
        #[allow(dead_code)]
        type SystemConfigDbWeight = $crate::pallet_tests::DbWeight;
        #[allow(dead_code)]
        type SystemConfigBlockWeights = $crate::pallet_tests::RuntimeBlockWeights;

        mod pallet_tests_system_config_impl {
            use super::*;

            $crate::impl_pallet_system_inner!($( $tokens )*);
        }
    };
}

#[macro_export]
macro_rules! impl_pallet_system_inner {
    ($runtime:ty$(,)?) => {
        impl frame_system::Config for $runtime {
            type BaseCallFilter = frame_support::traits::Everything;
            type BlockWeights = SystemConfigBlockWeights;
            type BlockLength = ();
            type DbWeight = SystemConfigDbWeight;
            type RuntimeOrigin = RuntimeOrigin;
            type RuntimeCall = RuntimeCall;
            type Index = u64;
            type BlockNumber = BlockNumber;
            type Hash = H256;
            type Hashing = BlakeTwo256;
            type AccountId = AccountId;
            type Lookup = IdentityLookup<Self::AccountId>;
            type Header = generic::Header<BlockNumber, BlakeTwo256>;
            type RuntimeEvent = RuntimeEvent;
            type BlockHashCount = BlockHashCount;
            type Version = ();
            type PalletInfo = PalletInfo;
            type AccountData = pallet_balances::AccountData<Balance>;
            type OnNewAccount = ();
            type OnKilledAccount = ();
            type SystemWeightInfo = ();
            type SS58Prefix = $crate::pallet_tests::SS58Prefix;
            type OnSetCode = ();
            type MaxConsumers = frame_support::traits::ConstU32<16>;
        }
    };

    ($runtime:ty, DbWeight = $db_weight:ty $(, $( $rest:tt )*)?) => {
        type SystemConfigDbWeight = $db_weight;

        $crate::impl_pallet_system_inner!($runtime, $($( $rest )*)?);
    };

    ($runtime:ty, BlockWeights = $block_weights:ty $(, $( $rest:tt )*)?) => {
        type SystemConfigBlockWeights = $block_weights;

        $crate::impl_pallet_system_inner!($runtime, $($( $rest )*)?);
    };
}

#[macro_export]
macro_rules! impl_pallet_timestamp {
    ($runtime:ty) => {
        impl pallet_timestamp::Config for Test {
            type Moment = u64;
            type OnTimestampSet = ();
            type MinimumPeriod = $crate::pallet_tests::MinimumPeriod;
            type WeightInfo = ();
        }
    };
}

#[macro_export]
macro_rules! impl_pallet_authorship {
    ($runtime:ty, EventHandler = $event_handler:ty) => {
        pub struct FixedBlockAuthor;

        impl FindAuthor<AccountId> for FixedBlockAuthor {
            fn find_author<'a, I: 'a>(_: I) -> Option<AccountId> {
                Some(BLOCK_AUTHOR)
            }
        }

        impl pallet_authorship::Config for $runtime {
            type FindAuthor = FixedBlockAuthor;
            type EventHandler = $event_handler;
        }
    };

    ($runtime:ty) => {
        $crate::impl_pallet_authorship!($runtime, EventHandler = ());
    };
}

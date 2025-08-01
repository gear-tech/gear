// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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

//! Autogenerated weights for pallet_balances
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 43.0.0
//! DATE: 2025-07-23, STEPS: `50`, REPEAT: `20`, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! WORST CASE MAP SIZE: `1000000`
//! CPU: `Intel(R) Xeon(R) Platinum 8375C CPU @ 2.90GHz`
//! EXECUTION: , WASM-EXECUTION: Compiled, CHAIN: None, DB CACHE: 1024

// Executed Command:
// ./target/production/gear benchmark pallet --runtime=./target/production/wbuild/vara-runtime/vara_runtime.compact.compressed.wasm --genesis-builder=runtime --genesis-builder-preset=development --steps=50 --repeat=20 --pallet=pallet_balances --extrinsic=* --heap-pages=4096 --output=./scripts/benchmarking/weights-output/pallet_balances.rs --template=.maintain/frame-weight-template.hbs

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(clippy::unnecessary_cast)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use sp_std::marker::PhantomData;

/// Weight functions needed for pallet_balances.
pub trait WeightInfo {
    fn transfer_allow_death() -> Weight;
    fn transfer_keep_alive() -> Weight;
    fn force_set_balance_creating() -> Weight;
    fn force_set_balance_killing() -> Weight;
    fn force_transfer() -> Weight;
    fn transfer_all() -> Weight;
    fn force_unreserve() -> Weight;
    fn upgrade_accounts(u: u32, ) -> Weight;
    fn force_adjust_total_issuance() -> Weight;
    fn burn_allow_death() -> Weight;
    fn burn_keep_alive() -> Weight;
}

/// Weights for pallet_balances using the Gear node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> pallet_balances::WeightInfo for SubstrateWeight<T> {
    fn transfer_allow_death() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `103`
        //  Estimated: `6196`
        // Minimum execution time: 71_376_000 picoseconds.
        Weight::from_parts(72_649_000, 6196)
            .saturating_add(T::DbWeight::get().reads(2_u64))
            .saturating_add(T::DbWeight::get().writes(2_u64))
    }
    fn transfer_keep_alive() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `0`
        //  Estimated: `3593`
        // Minimum execution time: 42_260_000 picoseconds.
        Weight::from_parts(43_567_000, 3593)
            .saturating_add(T::DbWeight::get().reads(1_u64))
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }
    fn force_set_balance_creating() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `103`
        //  Estimated: `3593`
        // Minimum execution time: 17_343_000 picoseconds.
        Weight::from_parts(17_695_000, 3593)
            .saturating_add(T::DbWeight::get().reads(1_u64))
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }
    fn force_set_balance_killing() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `103`
        //  Estimated: `3593`
        // Minimum execution time: 24_685_000 picoseconds.
        Weight::from_parts(25_423_000, 3593)
            .saturating_add(T::DbWeight::get().reads(1_u64))
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }
    fn force_transfer() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `206`
        //  Estimated: `8799`
        // Minimum execution time: 74_787_000 picoseconds.
        Weight::from_parts(76_194_000, 8799)
            .saturating_add(T::DbWeight::get().reads(3_u64))
            .saturating_add(T::DbWeight::get().writes(3_u64))
    }
    fn transfer_all() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `0`
        //  Estimated: `3593`
        // Minimum execution time: 53_538_000 picoseconds.
        Weight::from_parts(54_481_000, 3593)
            .saturating_add(T::DbWeight::get().reads(1_u64))
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }
    fn force_unreserve() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `103`
        //  Estimated: `3593`
        // Minimum execution time: 20_424_000 picoseconds.
        Weight::from_parts(20_857_000, 3593)
            .saturating_add(T::DbWeight::get().reads(1_u64))
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }
    /// The range of component `u` is `[1, 1000]`.
    fn upgrade_accounts(u: u32, ) -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `0 + u * (136 ±0)`
        //  Estimated: `990 + u * (2603 ±0)`
        // Minimum execution time: 20_293_000 picoseconds.
        Weight::from_parts(20_525_000, 990)
            // Standard Error: 12_401
            .saturating_add(Weight::from_parts(15_914_080, 0).saturating_mul(u.into()))
            .saturating_add(T::DbWeight::get().reads((1_u64).saturating_mul(u.into())))
            .saturating_add(T::DbWeight::get().writes((1_u64).saturating_mul(u.into())))
            .saturating_add(Weight::from_parts(0, 2603).saturating_mul(u.into()))
    }
    fn force_adjust_total_issuance() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `0`
        //  Estimated: `0`
        // Minimum execution time: 6_547_000 picoseconds.
        Weight::from_parts(6_743_000, 0)
    }
    fn burn_allow_death() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `103`
        //  Estimated: `3593`
        // Minimum execution time: 52_675_000 picoseconds.
        Weight::from_parts(53_323_000, 3593)
            .saturating_add(T::DbWeight::get().reads(1_u64))
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }
    fn burn_keep_alive() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `0`
        //  Estimated: `0`
        // Minimum execution time: 23_865_000 picoseconds.
        Weight::from_parts(24_378_000, 0)
    }
}

// For backwards compatibility and tests
impl WeightInfo for () {
    fn transfer_allow_death() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `103`
        //  Estimated: `6196`
        // Minimum execution time: 71_376_000 picoseconds.
        Weight::from_parts(72_649_000, 6196)
            .saturating_add(RocksDbWeight::get().reads(2_u64))
            .saturating_add(RocksDbWeight::get().writes(2_u64))
    }
    fn transfer_keep_alive() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `0`
        //  Estimated: `3593`
        // Minimum execution time: 42_260_000 picoseconds.
        Weight::from_parts(43_567_000, 3593)
            .saturating_add(RocksDbWeight::get().reads(1_u64))
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }
    fn force_set_balance_creating() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `103`
        //  Estimated: `3593`
        // Minimum execution time: 17_343_000 picoseconds.
        Weight::from_parts(17_695_000, 3593)
            .saturating_add(RocksDbWeight::get().reads(1_u64))
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }
    fn force_set_balance_killing() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `103`
        //  Estimated: `3593`
        // Minimum execution time: 24_685_000 picoseconds.
        Weight::from_parts(25_423_000, 3593)
            .saturating_add(RocksDbWeight::get().reads(1_u64))
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }
    fn force_transfer() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `206`
        //  Estimated: `8799`
        // Minimum execution time: 74_787_000 picoseconds.
        Weight::from_parts(76_194_000, 8799)
            .saturating_add(RocksDbWeight::get().reads(3_u64))
            .saturating_add(RocksDbWeight::get().writes(3_u64))
    }
    fn transfer_all() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `0`
        //  Estimated: `3593`
        // Minimum execution time: 53_538_000 picoseconds.
        Weight::from_parts(54_481_000, 3593)
            .saturating_add(RocksDbWeight::get().reads(1_u64))
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }
    fn force_unreserve() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `103`
        //  Estimated: `3593`
        // Minimum execution time: 20_424_000 picoseconds.
        Weight::from_parts(20_857_000, 3593)
            .saturating_add(RocksDbWeight::get().reads(1_u64))
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }
    /// The range of component `u` is `[1, 1000]`.
    fn upgrade_accounts(u: u32, ) -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `0 + u * (136 ±0)`
        //  Estimated: `990 + u * (2603 ±0)`
        // Minimum execution time: 20_293_000 picoseconds.
        Weight::from_parts(20_525_000, 990)
            // Standard Error: 12_401
            .saturating_add(Weight::from_parts(15_914_080, 0).saturating_mul(u.into()))
            .saturating_add(RocksDbWeight::get().reads((1_u64).saturating_mul(u.into())))
            .saturating_add(RocksDbWeight::get().writes((1_u64).saturating_mul(u.into())))
            .saturating_add(Weight::from_parts(0, 2603).saturating_mul(u.into()))
    }
    fn force_adjust_total_issuance() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `0`
        //  Estimated: `0`
        // Minimum execution time: 6_547_000 picoseconds.
        Weight::from_parts(6_743_000, 0)
    }
    fn burn_allow_death() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `103`
        //  Estimated: `3593`
        // Minimum execution time: 52_675_000 picoseconds.
        Weight::from_parts(53_323_000, 3593)
            .saturating_add(RocksDbWeight::get().reads(1_u64))
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }
    fn burn_keep_alive() -> Weight {
        // Proof Size summary in bytes:
        //  Measured:  `0`
        //  Estimated: `0`
        // Minimum execution time: 23_865_000 picoseconds.
        Weight::from_parts(24_378_000, 0)
    }
}

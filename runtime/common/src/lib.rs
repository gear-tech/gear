// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

mod apis;
pub mod constants;
pub mod weights;

use sp_runtime::traits::Get;

use frame_support::{
    pallet_prelude::DispatchClass,
    parameter_types,
    traits::{Currency, Imbalance, OnUnbalanced},
    weights::{
        constants::{BlockExecutionWeight, ExtrinsicBaseWeight},
        Weight,
    },
};
use frame_system::limits::BlockWeights;
use runtime_primitives::{AccountId, BlockNumber};
use sp_runtime::Perbill;

/// We assume that ~3% of the block weight is consumed by `on_initialize` handlers.
/// This is used to limit the maximal weight of a single extrinsic.
///
/// Mostly we don't produce any calculations in `on_initialize` hook,
/// so it's safe to reduce from default 10 to custom 3 percents.
pub const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(3);

pub const NORMAL_DISPATCH_LENGTH_RATIO_NUM: u8 = 50;
pub const NORMAL_DISPATCH_LENGTH_RATIO: Perbill =
    Perbill::from_percent(NORMAL_DISPATCH_LENGTH_RATIO_NUM as u32);

pub const NORMAL_DISPATCH_WEIGHT_RATIO_NUM: u8 = 25;
pub const GAS_LIMIT_MIN_PERCENTAGE_NUM: u8 = 100 - NORMAL_DISPATCH_WEIGHT_RATIO_NUM;

// Extrinsics with DispatchClass::Normal only account for user messages
// TODO: consider making the normal extrinsics share adjustable in runtime
pub const NORMAL_DISPATCH_WEIGHT_RATIO: Perbill =
    Perbill::from_percent(NORMAL_DISPATCH_WEIGHT_RATIO_NUM as u32);

/// Returns common for gear protocol `BlockWeights` depend on given max block weight.
pub fn block_weights_for(maximum_block_weight: Weight) -> BlockWeights {
    BlockWeights::builder()
        .base_block(BlockExecutionWeight::get())
        .for_class(DispatchClass::all(), |weights| {
            weights.base_extrinsic = ExtrinsicBaseWeight::get();
        })
        .for_class(DispatchClass::Normal, |weights| {
            weights.max_total = Some(NORMAL_DISPATCH_WEIGHT_RATIO * maximum_block_weight);
        })
        .for_class(DispatchClass::Operational, |weights| {
            weights.max_total = Some(maximum_block_weight);
            // Operational transactions have some extra reserved space, so that they
            // are included even if block reached `MAXIMUM_BLOCK_WEIGHT`.
            weights.reserved =
                Some(maximum_block_weight - NORMAL_DISPATCH_WEIGHT_RATIO * maximum_block_weight);
        })
        .avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
        .build_or_panic()
}

parameter_types! {
    pub const BlockHashCount: BlockNumber = 2400;
}

pub const VALUE_PER_GAS: u128 = 6;

pub type NegativeImbalance<T> = <pallet_balances::Pallet<T> as Currency<
    <T as frame_system::Config>::AccountId,
>>::NegativeImbalance;

/// Logic for the author to get a portion of fees.
pub struct ToAuthor<R>(sp_std::marker::PhantomData<R>);
impl<R> OnUnbalanced<NegativeImbalance<R>> for ToAuthor<R>
where
    R: pallet_balances::Config + pallet_authorship::Config,
    <R as frame_system::Config>::AccountId: From<AccountId>,
    <R as frame_system::Config>::AccountId: Into<AccountId>,
{
    fn on_nonzero_unbalanced(amount: NegativeImbalance<R>) {
        if let Some(author) = <pallet_authorship::Pallet<R>>::author() {
            <pallet_balances::Pallet<R>>::resolve_creating(&author, amount);
        }
    }
}

pub struct DealWithFees<R>(sp_std::marker::PhantomData<R>);
impl<R> OnUnbalanced<NegativeImbalance<R>> for DealWithFees<R>
where
    R: pallet_balances::Config
        + pallet_treasury::Config
        + pallet_authorship::Config
        + pallet_gear_bank::Config,
    pallet_treasury::Pallet<R>: OnUnbalanced<NegativeImbalance<R>>,
    <R as frame_system::Config>::AccountId: From<AccountId>,
    <R as frame_system::Config>::AccountId: Into<AccountId>,
{
    fn on_unbalanceds<B>(mut fees_then_tips: impl Iterator<Item = NegativeImbalance<R>>) {
        use pallet_treasury::Pallet as Treasury;

        if let Some(fees) = fees_then_tips.next() {
            let split_tx_fee_ratio = R::SplitTxFeeRatio::get();
            if let Some(split_tx_fee_ratio) = split_tx_fee_ratio {
                // for fees, SplitTxFeeRatio to treasury else to author
                let (mut to_author, to_treasury) =
                    fees.ration(100 - split_tx_fee_ratio, split_tx_fee_ratio);
                if let Some(tips) = fees_then_tips.next() {
                    // for tips, if any, 100% to author
                    tips.merge_into(&mut to_author);
                }
                <Treasury<R> as OnUnbalanced<_>>::on_unbalanced(to_treasury);
                <ToAuthor<R> as OnUnbalanced<_>>::on_unbalanced(to_author);
            } else {
                let mut to_author = fees;
                if let Some(tips) = fees_then_tips.next() {
                    // for tips, if any, 100% to author
                    tips.merge_into(&mut to_author);
                }
                <ToAuthor<R> as OnUnbalanced<_>>::on_unbalanced(to_author);
            }
        }
    }
}

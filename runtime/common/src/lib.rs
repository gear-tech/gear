// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use frame_support::{
    parameter_types,
    traits::{Currency, OnUnbalanced},
};
use runtime_primitives::{AccountId, Balance, BlockNumber};
use sp_runtime::Perbill;

/// We assume that ~10% of the block weight is consumed by `on_initialize` handlers.
/// This is used to limit the maximal weight of a single extrinsic.
pub const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(10);
pub const NORMAL_DISPATCH_RATIO_NUM: u8 = 25;
pub const GAS_LIMIT_MIN_PERCENTAGE_NUM: u8 = 100 - NORMAL_DISPATCH_RATIO_NUM;

// Extrinsics with DispatchClass::Normal only account for user messages
// TODO: consider making the normal extrinsics share adjustable in runtime
pub const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(NORMAL_DISPATCH_RATIO_NUM as u32);

parameter_types! {
    pub const BlockHashCount: BlockNumber = 2400;
}

pub struct GasConverter;
impl gear_common::GasPrice for GasConverter {
    type Balance = Balance;
}

pub type NegativeImbalance<T> = <pallet_balances::Pallet<T> as Currency<
    <T as frame_system::Config>::AccountId,
>>::NegativeImbalance;

pub struct DealWithFees<R>(sp_std::marker::PhantomData<R>);
impl<R> OnUnbalanced<NegativeImbalance<R>> for DealWithFees<R>
where
    R: pallet_balances::Config + pallet_authorship::Config,
    <R as frame_system::Config>::AccountId: From<AccountId>,
    <R as frame_system::Config>::AccountId: Into<AccountId>,
{
    fn on_unbalanceds<B>(mut fees_then_tips: impl Iterator<Item = NegativeImbalance<R>>) {
        if let Some(fees) = fees_then_tips.next() {
            if let Some(author) = <pallet_authorship::Pallet<R>>::author() {
                <pallet_balances::Pallet<R>>::resolve_creating(&author, fees);
            }
            if let Some(tips) = fees_then_tips.next() {
                if let Some(author) = <pallet_authorship::Pallet<R>>::author() {
                    <pallet_balances::Pallet<R>>::resolve_creating(&author, tips);
                }
            }
        }
    }
}

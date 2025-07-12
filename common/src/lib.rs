// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]

#[macro_use]
extern crate gear_common_codegen;

pub mod event;
pub mod scheduler;
pub mod storage;

pub mod code_storage;
pub use code_storage::{CodeStorage, Error as CodeStorageError};

pub mod program_storage;
pub use program_storage::{Error as ProgramStorageError, ProgramStorage};

pub mod gas_provider;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(feature = "std")]
pub mod pallet_tests;

use core::fmt;
use frame_support::{
    pallet_prelude::MaxEncodedLen,
    sp_runtime::{
        self,
        generic::{CheckedExtrinsic, UncheckedExtrinsic},
        traits::Dispatchable,
    },
    traits::Get,
};
pub use gear_core::{
    ids::{ActorId, CodeId, MessageId, ReservationId},
    memory::PageBuf,
    pages::GearPage,
    program::{ActiveProgram, MemoryInfix, Program},
};
use primitive_types::H256;
use sp_arithmetic::traits::{BaseArithmetic, Saturating, UniqueSaturatedInto, Unsigned};
use sp_runtime::{
    codec::{self, Decode, Encode},
    scale_info::{self, TypeInfo},
};
use sp_std::{collections::btree_map::BTreeMap, prelude::*};

use storage::ValueStorage;
extern crate alloc;

pub use gas_provider::{
    LockId, LockableTree, Provider as GasProvider, ReservableTree, Tree as GasTree,
};

/// Type alias for gas entity.
pub type Gas = u64;

/// NOTE: Implementation of this for `u64` places bytes from idx=0.
pub trait Origin: Sized {
    fn into_origin(self) -> H256;
    fn from_origin(val: H256) -> Self;
    fn cast<T: Origin>(self) -> T {
        T::from_origin(self.into_origin())
    }
}

impl Origin for u64 {
    fn into_origin(self) -> H256 {
        let mut result = H256::zero();
        result[0..8].copy_from_slice(&self.to_le_bytes());
        result
    }

    fn from_origin(v: H256) -> Self {
        // h256 -> u64 should not be used anywhere other than in tests!
        let mut val = [0u8; 8];
        val.copy_from_slice(&v[0..8]);
        Self::from_le_bytes(val)
    }
}

impl Origin for sp_runtime::AccountId32 {
    fn into_origin(self) -> H256 {
        H256::from(self.as_ref())
    }

    fn from_origin(v: H256) -> Self {
        Self::new(v.0)
    }
}

impl Origin for H256 {
    fn into_origin(self) -> H256 {
        self
    }

    fn from_origin(val: H256) -> Self {
        val
    }
}

impl Origin for MessageId {
    fn into_origin(self) -> H256 {
        H256(self.into())
    }

    fn from_origin(val: H256) -> Self {
        val.to_fixed_bytes().into()
    }
}

impl Origin for ActorId {
    fn into_origin(self) -> H256 {
        H256(self.into())
    }

    fn from_origin(val: H256) -> Self {
        val.to_fixed_bytes().into()
    }
}

impl Origin for CodeId {
    fn into_origin(self) -> H256 {
        H256(self.into())
    }

    fn from_origin(val: H256) -> Self {
        val.to_fixed_bytes().into()
    }
}

impl Origin for ReservationId {
    fn into_origin(self) -> H256 {
        H256(self.into())
    }

    fn from_origin(val: H256) -> Self {
        val.to_fixed_bytes().into()
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, MaxEncodedLen, TypeInfo,
)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
/// Type representing converter between gas and value in different relations.
pub enum GasMultiplier<Balance, Gas> {
    ValuePerGas(Balance),
    GasPerValue(Gas),
}

impl<Balance, Gas> GasMultiplier<Balance, Gas>
where
    Balance: BaseArithmetic + Copy + Unsigned,
    Gas: BaseArithmetic + Copy + Unsigned + UniqueSaturatedInto<Balance>,
{
    /// Converts given gas amount into its value equivalent.
    pub fn gas_to_value(&self, gas: Gas) -> Balance {
        let gas: Balance = gas.unique_saturated_into();

        match self {
            Self::ValuePerGas(multiplier) => gas.saturating_mul(*multiplier),
            Self::GasPerValue(_multiplier) => {
                // Consider option to return `(*cost*, *amount of gas to be bought*)`.
                unimplemented!("Currently unsupported that 1 Value > 1 Gas");
            }
        }
    }
}

impl<Balance, Gas> From<GasMultiplier<Balance, Gas>> for gsys::GasMultiplier
where
    Balance: Copy + UniqueSaturatedInto<gsys::Value>,
    Gas: Copy + UniqueSaturatedInto<gsys::Gas>,
{
    fn from(multiplier: GasMultiplier<Balance, Gas>) -> Self {
        match multiplier {
            GasMultiplier::ValuePerGas(multiplier) => {
                Self::from_value_per_gas((multiplier).unique_saturated_into())
            }
            GasMultiplier::GasPerValue(multiplier) => {
                Self::from_gas_per_value((multiplier).unique_saturated_into())
            }
        }
    }
}

pub trait QueueRunner {
    type Gas;

    fn run_queue(initial_gas: Self::Gas) -> Self::Gas;
}

/// Contains various limits for the block.
pub trait BlockLimiter {
    /// The maximum amount of gas that can be used within a single block.
    type BlockGasLimit: Get<Self::Balance>;

    /// Type representing a quantity of value.
    type Balance;

    /// Type manages a gas that is available at the moment of call.
    type GasAllowance: storage::Limiter<Value = Self::Balance>;
}

/// A trait whose purpose is to extract the `Call` variant of an extrinsic
pub trait ExtractCall<Call> {
    fn extract_call(&self) -> Call;
}

/// Implementation for unchecked extrinsic.
impl<Address, Call, Signature, Extra> ExtractCall<Call>
    for UncheckedExtrinsic<Address, Call, Signature, Extra>
where
    Call: Dispatchable + Clone,
    Extra: Decode,
{
    fn extract_call(&self) -> Call {
        self.function.clone()
    }
}

/// Implementation for checked extrinsic.
impl<Address, Call, Extra> ExtractCall<Call> for CheckedExtrinsic<Address, Call, Extra>
where
    Call: Dispatchable + Clone,
{
    fn extract_call(&self) -> Call {
        self.function.clone()
    }
}

/// Trait that the RuntimeApi should implement in order to allow deconstruction and reconstruction
/// to and from its components.
#[cfg(any(feature = "std", test))]
pub trait Deconstructable<Call> {
    type Params: Send;

    fn into_parts(self) -> (&'static Call, Self::Params);

    fn from_parts(call: &Call, params: Self::Params) -> Self;
}

/// Trait that is used to "delegate fee" by optionally changing
/// the payer target (account id) for the applied call.
pub trait DelegateFee<Call, Acc> {
    fn delegate_fee(call: &Call, who: &Acc) -> Option<Acc>;
}

impl<Call, Acc> DelegateFee<Call, Acc> for () {
    fn delegate_fee(_call: &Call, _who: &Acc) -> Option<Acc> {
        None
    }
}

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
// (issue #2531)
#![allow(deprecated)]

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

use core::fmt;
use frame_support::{
    codec::{self, Decode, Encode},
    dispatch::DispatchError,
    scale_info::{self, TypeInfo},
    sp_runtime::{
        self,
        generic::{CheckedExtrinsic, UncheckedExtrinsic},
        traits::{Dispatchable, SignedExtension},
    },
    traits::Get,
    weights::{ConstantMultiplier, Weight, WeightToFee},
};
use gear_core::{
    ids::{CodeId, MessageId, ProgramId},
    memory::{GearPage, PageBuf, WasmPage},
    message::DispatchKind,
    reservation::GasReservationMap,
};
use primitive_types::H256;
use sp_arithmetic::traits::{BaseArithmetic, Unsigned};
use sp_core::crypto::UncheckedFrom;
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    prelude::*,
};

use storage::ValueStorage;
extern crate alloc;

pub use gas_provider::{Provider as GasProvider, Tree as GasTree};

pub trait Origin: Sized {
    fn into_origin(self) -> H256;
    fn from_origin(val: H256) -> Self;
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
        sp_runtime::AccountId32::unchecked_from(v)
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

impl Origin for ProgramId {
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

pub trait GasPrice {
    type Balance: BaseArithmetic + From<u32> + Copy + Unsigned;

    type GasToBalanceMultiplier: Get<Self::Balance>;

    /// A price for the `gas` amount of gas.
    /// In general case, this doesn't necessarily has to be constant.
    fn gas_price(gas: u64) -> Self::Balance {
        ConstantMultiplier::<Self::Balance, Self::GasToBalanceMultiplier>::weight_to_fee(
            &Weight::from_ref_time(gas),
        )
    }
}

pub trait QueueRunner {
    type Gas;

    fn run_queue(initial_gas: Self::Gas) -> Self::Gas;
}

pub trait PaymentProvider<AccountId> {
    type Balance;

    fn withhold_reserved(
        source: H256,
        dest: &AccountId,
        amount: Self::Balance,
    ) -> Result<(), DispatchError>;
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

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum Program {
    Active(ActiveProgram),
    Exited(ProgramId),
    Terminated(ProgramId),
}

impl Program {
    pub fn is_active(&self) -> bool {
        matches!(self, Program::Active(_))
    }

    pub fn is_exited(&self) -> bool {
        matches!(self, Program::Exited(_))
    }

    pub fn is_terminated(&self) -> bool {
        matches!(self, Program::Terminated(_))
    }

    pub fn is_initialized(&self) -> bool {
        matches!(
            self,
            Program::Active(ActiveProgram {
                state: ProgramState::Initialized,
                ..
            })
        )
    }

    pub fn is_uninitialized(&self) -> bool {
        matches!(
            self,
            Program::Active(ActiveProgram {
                state: ProgramState::Uninitialized { .. },
                ..
            })
        )
    }
}

#[derive(Clone, Debug, derive_more::Display)]
#[display(fmt = "Program is not an active one")]
pub struct InactiveProgramError;

impl core::convert::TryFrom<Program> for ActiveProgram {
    type Error = InactiveProgramError;

    fn try_from(prog_with_status: Program) -> Result<ActiveProgram, Self::Error> {
        match prog_with_status {
            Program::Active(p) => Ok(p),
            _ => Err(InactiveProgramError),
        }
    }
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub struct ActiveProgram {
    /// Set of dynamic wasm page numbers, which are allocated by the program.
    pub allocations: BTreeSet<WasmPage>,
    /// Set of gear pages numbers, which has data in storage.
    pub pages_with_data: BTreeSet<GearPage>,
    pub gas_reservation_map: GasReservationMap,
    pub code_hash: H256,
    pub code_exports: BTreeSet<DispatchKind>,
    pub static_pages: WasmPage,
    pub state: ProgramState,
}

/// Enumeration contains variants for program state.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum ProgramState {
    /// `init` method of a program has not yet finished its execution so
    /// the program is not considered as initialized. All messages to such a
    /// program go to the wait list.
    /// `message_id` contains identifier of the initialization message.
    Uninitialized { message_id: MessageId },
    /// Program has been successfully initialized and can process messages.
    Initialized,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub struct CodeMetadata {
    pub author: H256,
    #[codec(compact)]
    pub block_number: u32,
}

impl CodeMetadata {
    pub fn new(author: H256, block_number: u32) -> Self {
        CodeMetadata {
            author,
            block_number,
        }
    }
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
    Extra: SignedExtension,
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

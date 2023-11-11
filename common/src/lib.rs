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

pub mod paused_program_storage;
pub use paused_program_storage::PausedProgramStorage;

pub mod gas_provider;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(feature = "std")]
pub mod pallet_tests;

use core::fmt;
use frame_support::{
    codec::{self, Decode, Encode},
    pallet_prelude::MaxEncodedLen,
    scale_info::{self, TypeInfo},
    sp_runtime::{
        self,
        generic::{CheckedExtrinsic, UncheckedExtrinsic},
        traits::{Dispatchable, SignedExtension},
    },
    traits::Get,
};
use gear_core::{
    ids::{CodeId, MessageId, ProgramId},
    memory::PageBuf,
    message::DispatchKind,
    pages::{GearPage, WasmPage},
    program::MemoryInfix,
    reservation::GasReservationMap,
};
use primitive_types::H256;
use sp_arithmetic::traits::{BaseArithmetic, One, Saturating, UniqueSaturatedInto, Unsigned};
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    prelude::*,
};

use storage::ValueStorage;
extern crate alloc;

pub use gas_provider::{
    LockId, LockableTree, Provider as GasProvider, ReservableTree, Tree as GasTree,
};

/// Type alias for gas entity.
pub type Gas = u64;

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

impl<Balance: One, Gas> Default for GasMultiplier<Balance, Gas> {
    fn default() -> Self {
        Self::ValuePerGas(One::one())
    }
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

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum Program<BlockNumber: Copy + Saturating> {
    Active(ActiveProgram<BlockNumber>),
    Exited(ProgramId),
    Terminated(ProgramId),
}

impl<BlockNumber: Copy + Saturating> Program<BlockNumber> {
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
}

#[derive(Clone, Debug, derive_more::Display)]
#[display(fmt = "Program is not an active one")]
pub struct InactiveProgramError;

impl<BlockNumber: Copy + Saturating> core::convert::TryFrom<Program<BlockNumber>>
    for ActiveProgram<BlockNumber>
{
    type Error = InactiveProgramError;

    fn try_from(prog_with_status: Program<BlockNumber>) -> Result<Self, Self::Error> {
        match prog_with_status {
            Program::Active(p) => Ok(p),
            _ => Err(InactiveProgramError),
        }
    }
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub struct ActiveProgram<BlockNumber: Copy + Saturating> {
    /// Set of dynamic wasm page numbers, which are allocated by the program.
    pub allocations: BTreeSet<WasmPage>,
    /// Set of gear pages numbers, which has data in storage.
    pub pages_with_data: BTreeSet<GearPage>,
    pub memory_infix: MemoryInfix,
    pub gas_reservation_map: GasReservationMap,
    pub code_hash: H256,
    pub code_exports: BTreeSet<DispatchKind>,
    pub static_pages: WasmPage,
    pub state: ProgramState,
    pub expiration_block: BlockNumber,
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

pub trait PaymentVoucher<AccountId, ProgramId, Balance> {
    type VoucherId;
    type Error;

    fn voucher_id(who: AccountId, program: ProgramId) -> Self::VoucherId;
}

impl<AccountId: Default, ProgramId, Balance> PaymentVoucher<AccountId, ProgramId, Balance> for () {
    type VoucherId = AccountId;
    type Error = &'static str;

    fn voucher_id(_who: AccountId, _program: ProgramId) -> Self::VoucherId {
        unimplemented!()
    }
}

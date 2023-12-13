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

//! # Gear Voucher Pallet
//!
//! The Gear Voucher Pallet provides functionality for alternative source of funds for
//! gas and transaction fees payment when sending messages to Contracts in Gear engine.
//! These funds can only be used for a specific purpose - much like a payment voucher.
//! Hence the pallet name.
//!
//! - [`Config`]
//! - [`Pallet`]
//!
//! ## Overview
//!
//! This pallet provides API for contract owners (or any other party for that matter)
//! to sponsor contract users by allocating some funds in such a way that these funds
//! can only be spent to pay for gas and transaction fees if a user sends a message
//! to the contract.
//!
//! The Gear Voucher Pallet provides functions for:
//! - Issuing a voucher for an account by any other account.
//! - Deriving a unique keyless account id for a pair (user, program) to hold allocated funds.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! * `issue` - Issue an `amount` tokens worth voucher for a `user` to be used to pay fees and gas when sending messages to `program`.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::items_after_test_module)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use frame_support::{
    pallet_prelude::*,
    traits::{Currency, ExistenceRequirement, ReservableCurrency, StorageVersion},
    PalletId,
};
use gear_core::ids::{MessageId, ProgramId};
pub use primitive_types::H256;
use sp_io::hashing::blake2_256;
use sp_runtime::traits::{StaticLookup, TrailingZeroInput};
use sp_std::{convert::TryInto, vec::Vec};
pub use weights::WeightInfo;

pub use pallet::*;

pub(crate) type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::storage::Mailbox;
    use frame_system::pallet_prelude::*;
    use gear_core::message::UserStoredMessage;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Currency implementation
        type Currency: ReservableCurrency<Self::AccountId>;

        /// The pallet id, used for deriving its sovereign account ID.
        #[pallet::constant]
        type PalletId: Get<PalletId>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;

        type CallsDispatcher: PrepaidCallsDispatcher<
            AccountId = Self::AccountId,
            Balance = BalanceOf<Self>,
        >;

        type Mailbox: Mailbox<Key1 = Self::AccountId, Key2 = MessageId, Value = UserStoredMessage>;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A new voucher issued.
        VoucherIssued {
            holder: T::AccountId,
            program: ProgramId,
            value: BalanceOf<T>,
        },
    }

    // Gas pallet error.
    #[pallet::error]
    pub enum Error<T> {
        InsufficientBalance,
        InvalidVoucher,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Issue a new voucher for a `user` to be used to pay for sending messages
        /// to `program_id` program.
        ///
        /// The dispatch origin for this call must be _Signed_.
        ///
        /// - `to`: The voucher holder account id.
        /// - `program`: The program id, messages to whom can be paid with the voucher.
        /// NOTE: the fact a program with such id exists in storage is not checked - it's
        /// a caller's responsibility to ensure the consistency of the input parameters.
        /// - `amount`: The voucher amount.
        ///
        /// ## Complexity
        /// O(Z + C) where Z is the length of the call and C its execution weight.
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::issue())]
        pub fn issue(
            origin: OriginFor<T>,
            to: AccountIdLookupOf<T>,
            program: ProgramId,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            let to = T::Lookup::lookup(to)?;

            // Generate unique account id corresponding to the pair (user, program)
            let voucher_id = Self::voucher_id(&to, &program);

            // Transfer funds to the keyless account
            T::Currency::transfer(&who, &voucher_id, value, ExistenceRequirement::KeepAlive)
                .map_err(|e| {
                    log::debug!("Failed to transfer funds to the voucher account: {:?}", e);
                    Error::<T>::InsufficientBalance
                })?;

            Self::deposit_event(Event::VoucherIssued {
                holder: to,
                program,
                value,
            });

            Ok(().into())
        }

        /// Dispatch allowed with voucher call.
        #[pallet::call_index(1)]
        #[pallet::weight(T::CallsDispatcher::weight(call))]
        pub fn call(
            origin: OriginFor<T>,
            call: PrepaidCall<BalanceOf<T>>,
        ) -> DispatchResultWithPostInfo {
            let origin = ensure_signed(origin)?;

            let sponsor = Self::sponsor_of(&origin, &call).ok_or(Error::<T>::InvalidVoucher)?;

            T::CallsDispatcher::dispatch(origin, sponsor, call)
        }
    }

    impl<T: Config> Pallet<T> {
        /// Derive a synthesized account ID from an account ID and a program ID.
        pub fn voucher_id(who: &T::AccountId, program_id: &ProgramId) -> T::AccountId {
            let entropy = (b"modlpy/voucher__", who, program_id).using_encoded(blake2_256);
            Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
                .expect("infinite length input; no invalid inputs for type; qed")
        }

        /// Return synthesized account ID based on call data.
        pub fn sponsor_of(
            who: &T::AccountId,
            call: &PrepaidCall<BalanceOf<T>>,
        ) -> Option<T::AccountId> {
            match call {
                PrepaidCall::SendMessage { destination, .. } => {
                    Some(Self::voucher_id(who, destination))
                }
                PrepaidCall::SendReply { reply_to_id, .. } => T::Mailbox::peek(who, reply_to_id)
                    .map(|stored_message| Self::voucher_id(who, &stored_message.source())),
            }
        }
    }
}

#[derive(Debug, Clone, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrepaidCall<Balance> {
    SendMessage {
        destination: ProgramId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: Balance,
        keep_alive: bool,
    },
    SendReply {
        reply_to_id: MessageId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: Balance,
        keep_alive: bool,
    },
}

pub trait PrepaidCallsDispatcher {
    type AccountId;
    type Balance;

    fn weight(call: &PrepaidCall<Self::Balance>) -> Weight;

    fn dispatch(
        account_id: Self::AccountId,
        sponsor_id: Self::AccountId,
        call: PrepaidCall<Self::Balance>,
    ) -> DispatchResultWithPostInfo;
}

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

mod internal;

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

pub use internal::*;
pub use pallet::*;

type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
pub(crate) type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::{
        storage::{Mailbox, ValueStorage},
        Origin,
    };
    use frame_system::pallet_prelude::*;
    use gear_core::message::UserStoredMessage;
    use sp_runtime::Saturating;

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

        /// Prepaid calls executor.
        type CallsDispatcher: PrepaidCallsDispatcher<
            AccountId = Self::AccountId,
            Balance = BalanceOf<Self>,
        >;

        /// Mailbox to extract destination for some prepaid cases (e.g. `Gear::send_reply`).
        type Mailbox: Mailbox<Key1 = Self::AccountId, Key2 = MessageId, Value = UserStoredMessage>;

        /// Maximal amount of programs to be specified to interact with.
        #[pallet::constant]
        type MaxProgramsAmount: Get<u8>;
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

        /// Revokable voucher (v2) has been updated.
        VoucherUpdated { voucher_id: VoucherId },

        /// Revokable voucher (v2) has been issued.
        RevokableVoucherIssued { voucher_id: VoucherId },
    }

    // Gas pallet error.
    #[pallet::error]
    pub enum Error<T> {
        BalanceTransfer,
        InexistentVoucher,
        VoucherExpired,
        IrrevocableYet,
        BadOrigin,
        MaxProgramsLimitExceeded,
        UnknownDestination,
    }

    // Private storage for amount of messages sent.
    #[pallet::storage]
    type Issued<T> = StorageValue<_, u64>;

    // Public wrap of the amount of messages sent.
    common::wrap_storage_value!(storage: Issued, name: IssuedWrap, value: u64);

    #[pallet::storage]
    // TODO (breathx): change to spender/voucher_id -> voucher data
    pub type Vouchers<T> = StorageDoubleMap<
        _,
        Identity,
        AccountIdOf<T>,
        Identity,
        VoucherId,
        VoucherInfo<AccountIdOf<T>, BlockNumberFor<T>>,
    >;

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
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
                .map_err(|_| Error::<T>::BalanceTransfer)?;

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

            let sponsor = Self::sponsor_of(&origin, &call).ok_or(Error::<T>::InexistentVoucher)?;

            T::CallsDispatcher::dispatch(origin, sponsor, call)
        }

        #[pallet::call_index(2)]
        #[pallet::weight(Weight::zero())] // TODO (breathx)
        pub fn new_issue(
            origin: OriginFor<T>,
            spender: AccountIdOf<T>,
            balance: BalanceOf<T>,
            programs: Option<Vec<ProgramId>>,
            validity: BlockNumberFor<T>,
        ) -> DispatchResultWithPostInfo {
            let owner = ensure_signed(origin)?;

            if let Some(ref programs) = programs {
                ensure!(
                    programs.len() <= T::MaxProgramsAmount::get().into(),
                    Error::<T>::MaxProgramsLimitExceeded
                )
            }

            let voucher_id = VoucherId::generate::<T>();

            T::Currency::transfer(
                &owner,
                &voucher_id.cast(),
                balance,
                ExistenceRequirement::KeepAlive,
            )
            .map_err(|_| Error::<T>::BalanceTransfer)?;

            let validity = <frame_system::Pallet<T>>::block_number().saturating_add(validity);

            let voucher_info = VoucherInfo {
                owner,
                programs,
                validity,
            };

            Vouchers::<T>::insert(spender, voucher_id, voucher_info);

            Self::deposit_event(Event::RevokableVoucherIssued { voucher_id });

            Ok(().into())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(Weight::zero())] // TODO (breathx)
        pub fn revoke(
            origin: OriginFor<T>,
            spender: AccountIdOf<T>,
            voucher_id: VoucherId,
        ) -> DispatchResultWithPostInfo {
            let origin = ensure_signed(origin)?;

            let Some(voucher) = Vouchers::<T>::take(spender, voucher_id) else {
                return Err(Error::<T>::InexistentVoucher.into());
            };

            // TODO (breathx/consider): should anyone be eligible to revoke?
            ensure!(voucher.owner == origin, Error::<T>::BadOrigin);

            ensure!(
                <frame_system::Pallet<T>>::block_number() >= voucher.validity,
                Error::<T>::IrrevocableYet
            );

            let voucher_id_acc = voucher_id.cast();

            T::Currency::transfer(
                &voucher_id_acc,
                &origin,
                T::Currency::free_balance(&voucher_id_acc),
                ExistenceRequirement::AllowDeath,
            )
            .map_err(|_| Error::<T>::BalanceTransfer)?;

            Ok(().into())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(Weight::zero())] // TODO (breathx)
        pub fn update(
            origin: OriginFor<T>,
            spender: AccountIdOf<T>,
            voucher_id: VoucherId,
            move_ownership: Option<AccountIdOf<T>>,
            balance_top_up: Option<BalanceOf<T>>,
            append_programs: Option<Vec<ProgramId>>,
            prolong_validity: Option<BlockNumberFor<T>>,
        ) -> DispatchResultWithPostInfo {
            let origin = ensure_signed(origin)?;

            let Some(mut voucher) = Vouchers::<T>::get(spender.clone(), voucher_id) else {
                return Err(Error::<T>::InexistentVoucher.into());
            };

            ensure!(voucher.owner == origin, Error::<T>::BadOrigin);

            if let Some(owner) = move_ownership {
                voucher.owner = owner;
            }

            if let Some(amount) = balance_top_up {
                T::Currency::transfer(
                    &origin,
                    &voucher_id.cast(),
                    amount,
                    ExistenceRequirement::AllowDeath,
                )
                .map_err(|_| Error::<T>::BalanceTransfer)?;
            }

            if let Some(mut extra_programs) = append_programs {
                if let Some(ref mut programs) = voucher.programs {
                    ensure!(
                        programs.len().saturating_add(extra_programs.len())
                            <= T::MaxProgramsAmount::get().into(),
                        Error::<T>::MaxProgramsLimitExceeded
                    );

                    programs.append(&mut extra_programs)
                }
            }

            if let Some(duration) = prolong_validity {
                voucher.validity = voucher
                    .validity
                    .max(<frame_system::Pallet<T>>::block_number())
                    .saturating_add(duration);
            }

            Vouchers::<T>::insert(spender, voucher_id, voucher);

            Self::deposit_event(Event::VoucherUpdated { voucher_id });

            Ok(().into())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::CallsDispatcher::weight(call))] // TODO (breathx)
        pub fn call_new(
            origin: OriginFor<T>,
            voucher_id: VoucherId,
            call: PrepaidCall<BalanceOf<T>>,
        ) -> DispatchResultWithPostInfo {
            let origin = ensure_signed(origin)?;

            let Some(voucher) = Vouchers::<T>::get(origin.clone(), voucher_id) else {
                return Err(Error::<T>::InexistentVoucher.into());
            };

            ensure!(
                <frame_system::Pallet<T>>::block_number() < voucher.validity,
                Error::<T>::VoucherExpired
            );

            let destination =
                Self::destination_program(&origin, &call).ok_or(Error::<T>::UnknownDestination)?;

            if voucher.contains(destination) {
                unreachable!("Should be filtered in `SignedExt`");
            }

            T::CallsDispatcher::dispatch(origin, voucher_id.cast(), call)
        }
    }
}

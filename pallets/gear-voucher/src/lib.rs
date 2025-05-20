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

//! # Gear Voucher Pallet
//!
//! The Gear Voucher Pallet provides functionality for alternative source of funds for
//! gas and transaction fees payment when sending messages to programs in Gear engine.
//! These funds can only be used for a specific purpose - much like a payment voucher.
//! Hence the pallet name.
//!
//! - [`Config`]
//! - [`Pallet`]
//!
//! ## Overview
//!
//! This pallet provides API for program owners (or any other party for that matter)
//! to sponsor program users by allocating some funds in such a way that these funds
//! can only be spent to pay for gas and transaction fees if a user sends a message
//! to the program.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::items_after_test_module)]
#![allow(clippy::manual_inspect)]
#![allow(clippy::useless_conversion)]

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
use gear_core::ids::{ActorId, MessageId};
pub use primitive_types::H256;
use sp_std::{convert::TryInto, vec::Vec};
pub use weights::WeightInfo;

pub use internal::*;
pub use pallet::*;

type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

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
    use sp_runtime::{
        traits::{CheckedSub, One, Zero},
        SaturatedConversion, Saturating,
    };
    use sp_std::collections::btree_set::BTreeSet;

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

        /// Minimal duration in blocks voucher could be issued/prolonged for.
        #[pallet::constant]
        type MinDuration: Get<BlockNumberFor<Self>>;

        /// Maximal duration in blocks voucher could be issued/prolonged for.
        #[pallet::constant]
        type MaxDuration: Get<BlockNumberFor<Self>>;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    /// Pallet Gear Voucher event.
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Voucher has been issued.
        VoucherIssued {
            /// Account id of owner and manager of the voucher.
            owner: AccountIdOf<T>,
            /// Account id of user eligible to use the voucher.
            spender: AccountIdOf<T>,
            /// Voucher identifier.
            voucher_id: VoucherId,
        },

        /// Voucher has been revoked by owner.
        ///
        /// NOTE: currently means only "refunded".
        VoucherRevoked {
            /// Account id of the user whose voucher was revoked.
            spender: AccountIdOf<T>,
            /// Voucher identifier.
            voucher_id: VoucherId,
        },

        /// Voucher has been updated.
        VoucherUpdated {
            /// Account id of user whose voucher was updated.
            spender: AccountIdOf<T>,
            /// Voucher identifier.
            voucher_id: VoucherId,
            /// Optional field defining was the owner changed during update.
            new_owner: Option<AccountIdOf<T>>,
        },

        /// Voucher has been declined (set to expired state).
        VoucherDeclined {
            /// Account id of user who declined its own voucher.
            spender: AccountIdOf<T>,
            /// Voucher identifier.
            voucher_id: VoucherId,
        },
    }

    // Pallet Gear Voucher error.
    #[pallet::error]
    pub enum Error<T> {
        /// The origin is not eligible to execute call.
        BadOrigin,
        /// Error trying transfer balance to/from voucher account.
        BalanceTransfer,
        /// Destination program is not in whitelisted set for voucher.
        InappropriateDestination,
        /// Voucher with given identifier doesn't exist for given spender id.
        InexistentVoucher,
        /// Voucher still valid and couldn't be revoked.
        IrrevocableYet,
        /// Try to whitelist more programs than allowed.
        MaxProgramsLimitExceeded,
        /// Failed to query destination of the prepaid call.
        UnknownDestination,
        /// Voucher has expired and couldn't be used.
        VoucherExpired,
        /// Voucher issue/prolongation duration out of [min; max] constants.
        DurationOutOfBounds,
        /// Voucher update function tries to cut voucher ability of code upload.
        CodeUploadingEnabled,
        /// Voucher is disabled for code uploading, but requested.
        CodeUploadingDisabled,
    }

    /// Storage containing amount of the total vouchers issued.
    ///
    /// Used as nonce in voucher creation.
    #[pallet::storage]
    type Issued<T> = StorageValue<_, u64>;

    // Public wrap of the amount of issued vouchers.
    common::wrap_storage_value!(storage: Issued, name: IssuedWrap, value: u64);

    /// Double map storage containing data of the voucher,
    /// associated with some spender and voucher ids.
    #[pallet::storage]
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
        /// Issue a new voucher.
        ///
        /// Deposits event `VoucherIssued`, that contains `VoucherId` to be
        /// used by spender for balance-less on-chain interactions.
        ///
        /// Arguments:
        /// * spender:  user id that is eligible to use the voucher;
        /// * balance:  voucher balance could be used for transactions
        ///             fees and gas;
        /// * programs: pool of programs spender can interact with,
        ///             if None - means any program,
        ///             limited by Config param;
        /// * code_uploading:
        ///             allow voucher to be used as payer for `upload_code`
        ///             transactions fee;
        /// * duration: amount of blocks voucher could be used by spender
        ///             and couldn't be revoked by owner.
        ///             Must be out in [MinDuration; MaxDuration] constants.
        ///             Expiration block of the voucher calculates as:
        ///             current bn (extrinsic exec bn) + duration + 1.
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::issue())]
        pub fn issue(
            origin: OriginFor<T>,
            spender: AccountIdOf<T>,
            balance: BalanceOf<T>,
            programs: Option<BTreeSet<ActorId>>,
            code_uploading: bool,
            duration: BlockNumberFor<T>,
        ) -> DispatchResultWithPostInfo {
            // Ensuring origin.
            let owner = ensure_signed(origin)?;

            // Asserting duration validity.
            ensure!(
                T::MinDuration::get() <= duration && duration <= T::MaxDuration::get(),
                Error::<T>::DurationOutOfBounds
            );

            // Asserting amount of programs.
            if let Some(ref programs) = programs {
                ensure!(
                    programs.len() <= T::MaxProgramsAmount::get().saturated_into(),
                    Error::<T>::MaxProgramsLimitExceeded
                )
            }

            // Creating voucher id.
            let voucher_id = VoucherId::generate::<T>();

            // Transferring funds to voucher account.
            T::Currency::transfer(
                &owner,
                &voucher_id.cast(),
                balance,
                ExistenceRequirement::KeepAlive,
            )
            .map_err(|_| Error::<T>::BalanceTransfer)?;

            // Calculating expiration block.
            let expiry = <frame_system::Pallet<T>>::block_number()
                .saturating_add(duration)
                .saturating_add(One::one());

            // Aggregating vouchers data.
            let voucher_info = VoucherInfo {
                owner: owner.clone(),
                programs,
                code_uploading,
                expiry,
            };

            // Inserting voucher data into storage, associated with spender and voucher ids.
            Vouchers::<T>::insert(spender.clone(), voucher_id, voucher_info);

            // Depositing event.
            Self::deposit_event(Event::VoucherIssued {
                owner,
                spender,
                voucher_id,
            });

            Ok(().into())
        }

        /// Execute prepaid call with given voucher id.
        ///
        /// Arguments:
        /// * voucher_id: associated with origin existing vouchers id,
        ///               that should be used to pay for fees and gas
        ///               within the call;
        /// * call:       prepaid call that is requested to execute.
        #[pallet::call_index(1)]
        #[pallet::weight(T::CallsDispatcher::weight(call).saturating_add(T::DbWeight::get().reads(2)))]
        pub fn call(
            origin: OriginFor<T>,
            voucher_id: VoucherId,
            call: PrepaidCall<BalanceOf<T>>,
        ) -> DispatchResultWithPostInfo {
            // Ensuring origin.
            let origin = ensure_signed(origin)?;

            // Validating that origin (spender) can use given voucher for call.
            Self::validate_prepaid(origin.clone(), voucher_id, &call)?;

            // Dispatching of the call.
            T::CallsDispatcher::dispatch(origin, voucher_id.cast(), voucher_id, call)
        }

        /// Revoke existing voucher.
        ///
        /// This extrinsic revokes existing voucher, if current block is greater
        /// than expiration block of the voucher (it is no longer valid).
        ///
        /// Currently it means sending of all balance from voucher account to
        /// voucher owner without voucher removal from storage map, but this
        /// behavior may change in future, as well as the origin validation:
        /// only owner is able to revoke voucher now.
        ///
        /// Arguments:
        /// * spender:    account id of the voucher spender;
        /// * voucher_id: voucher id to be revoked.
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::revoke())]
        pub fn revoke(
            origin: OriginFor<T>,
            spender: AccountIdOf<T>,
            voucher_id: VoucherId,
        ) -> DispatchResultWithPostInfo {
            // Ensuring origin.
            let origin = ensure_signed(origin)?;

            // Querying voucher data.
            // NOTE: currently getting instead of taking value.
            let voucher = Vouchers::<T>::get(spender.clone(), voucher_id)
                .ok_or(Error::<T>::InexistentVoucher)?;

            // NOTE: currently ensuring that owner revokes voucher.
            ensure!(voucher.owner == origin, Error::<T>::BadOrigin);

            // Ensuring voucher is expired.
            ensure!(
                <frame_system::Pallet<T>>::block_number() >= voucher.expiry,
                Error::<T>::IrrevocableYet
            );

            // Casting voucher id into voucher account id.
            let voucher_id_acc = voucher_id.cast();

            // Querying voucher account id balance.
            let voucher_balance = T::Currency::free_balance(&voucher_id_acc);

            // If balance of the voucher account is not zero, than transferring
            // all the funds to voucher owner and depositing event
            // `VoucherRevoked`, otherwise call is Noop.
            if !voucher_balance.is_zero() {
                // Supposed to be infallible.
                //
                // Transferring all free balance.
                T::Currency::transfer(
                    &voucher_id_acc,
                    &voucher.owner,
                    voucher_balance,
                    ExistenceRequirement::AllowDeath,
                )
                .map_err(|_| Error::<T>::BalanceTransfer)?;

                // Depositing event.
                Self::deposit_event(Event::VoucherRevoked {
                    spender,
                    voucher_id,
                });
            }

            Ok(().into())
        }

        /// Update existing voucher.
        ///
        /// This extrinsic updates existing voucher: it can only extend vouchers
        /// rights in terms of balance, validity or programs to interact pool.
        ///
        /// Can only be called by the voucher owner.
        ///
        /// Arguments:
        /// * spender:          account id of the voucher spender;
        /// * voucher_id:       voucher id to be updated;
        /// * move_ownership:   optionally moves ownership to another account;
        /// * balance_top_up:   optionally top ups balance of the voucher from
        ///                     origins balance;
        /// * append_programs:  optionally extends pool of programs by
        ///                     `Some(programs_set)` passed or allows
        ///                     it to interact with any program by
        ///                     `None` passed;
        /// * code_uploading:   optionally allows voucher to be used to pay
        ///                     fees for `upload_code` extrinsics;
        /// * prolong_duration: optionally increases expiry block number.
        ///                     If voucher is expired, prolongs since current bn.
        ///                     Validity prolongation (since current block number
        ///                     for expired or since storage written expiry)
        ///                     should be in [MinDuration; MaxDuration], in other
        ///                     words voucher couldn't have expiry greater than
        ///                     current block number + MaxDuration.
        #[allow(clippy::too_many_arguments)]
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::update())]
        pub fn update(
            origin: OriginFor<T>,
            spender: AccountIdOf<T>,
            voucher_id: VoucherId,
            move_ownership: Option<AccountIdOf<T>>,
            balance_top_up: Option<BalanceOf<T>>,
            append_programs: Option<Option<BTreeSet<ActorId>>>,
            code_uploading: Option<bool>,
            prolong_duration: Option<BlockNumberFor<T>>,
        ) -> DispatchResultWithPostInfo {
            // Ensuring origin.
            let origin = ensure_signed(origin)?;

            // Querying voucher.
            let mut voucher = Vouchers::<T>::get(spender.clone(), voucher_id)
                .ok_or(Error::<T>::InexistentVoucher)?;

            // Ensuring origin is owner of the voucher.
            ensure!(voucher.owner == origin, Error::<T>::BadOrigin);

            // Flag of extrinsic Noop: if voucher needs update in storage.
            let mut updated = false;

            // Flattening move ownership back to current owner.
            let new_owner = move_ownership.filter(|addr| *addr != voucher.owner);

            // Flattening code uploading.
            let code_uploading = code_uploading.filter(|v| *v != voucher.code_uploading);

            // Flattening duration prolongation.
            let prolong_duration = prolong_duration.filter(|dur| !dur.is_zero());

            // Optionally updates voucher owner.
            if let Some(ref owner) = new_owner {
                voucher.owner = owner.clone();
                updated = true;
            }

            // Optionally top ups voucher balance.
            if let Some(amount) = balance_top_up.filter(|x| !x.is_zero()) {
                T::Currency::transfer(
                    &origin,
                    &voucher_id.cast(),
                    amount,
                    ExistenceRequirement::AllowDeath,
                )
                .map_err(|_| Error::<T>::BalanceTransfer)?;

                updated = true;
            }

            // Optionally extends whitelisted programs with amount validation.
            match append_programs {
                // Adding given destination set to voucher,
                // if it has destinations limit.
                Some(Some(mut extra_programs)) if voucher.programs.is_some() => {
                    let programs = voucher.programs.as_mut().expect("Infallible; qed");
                    let initial_len = programs.len();

                    programs.append(&mut extra_programs);

                    ensure!(
                        programs.len() <= T::MaxProgramsAmount::get().into(),
                        Error::<T>::MaxProgramsLimitExceeded
                    );

                    updated |= programs.len() != initial_len;
                }

                // Extending vouchers to unlimited destinations.
                Some(None) => updated |= voucher.programs.take().is_some(),

                // Noop.
                _ => (),
            }

            // Optionally enabling code uploading.
            if let Some(code_uploading) = code_uploading {
                ensure!(code_uploading, Error::<T>::CodeUploadingEnabled);

                voucher.code_uploading = true;
                updated = true;
            }

            // Optionally prolongs validity of the voucher.
            if let Some(duration) = prolong_duration {
                let current_bn = <frame_system::Pallet<T>>::block_number();

                let (expiry, duration) =
                    if let Some(period) = voucher.expiry.checked_sub(&current_bn) {
                        let expiry = voucher.expiry.saturating_add(duration);
                        let new_duration = period.saturating_add(duration);
                        (expiry, new_duration)
                    } else {
                        let expiry = current_bn
                            .saturating_add(duration)
                            .saturating_add(One::one());
                        (expiry, duration)
                    };

                // Asserting duration validity.
                ensure!(
                    T::MinDuration::get() <= duration && duration <= T::MaxDuration::get(),
                    Error::<T>::DurationOutOfBounds
                );

                voucher.expiry = expiry;
                updated = true;
            }

            // Check for Noop.
            if updated {
                // Inserting updated voucher back in storage.
                Vouchers::<T>::insert(spender.clone(), voucher_id, voucher);

                // Depositing event, containing data if owner was updated.
                Self::deposit_event(Event::VoucherUpdated {
                    spender,
                    voucher_id,
                    new_owner,
                });
            }

            Ok(().into())
        }

        /// Decline existing and not expired voucher.
        ///
        /// This extrinsic expires voucher of the caller, if it's still active,
        /// allowing it to be revoked.
        ///
        /// Arguments:
        /// * voucher_id:   voucher id to be declined.
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::decline())]
        pub fn decline(origin: OriginFor<T>, voucher_id: VoucherId) -> DispatchResultWithPostInfo {
            // Ensuring origin.
            let origin = ensure_signed(origin)?;

            // Querying voucher if its not expired.
            let mut voucher = Self::get_active_voucher(origin.clone(), voucher_id)?;

            // Set voucher into expired state.
            voucher.expiry = <frame_system::Pallet<T>>::block_number();

            // Updating voucher in storage.
            // TODO: consider revoke here once gas counting implemented (#3726).
            Vouchers::<T>::insert(origin.clone(), voucher_id, voucher);

            // Depositing event.
            Self::deposit_event(Event::VoucherDeclined {
                spender: origin,
                voucher_id,
            });

            Ok(().into())
        }
    }
}

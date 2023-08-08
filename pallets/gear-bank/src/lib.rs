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

//! # Gear Funds Pallet.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub use pallet::*;

use frame_support::traits::{Currency, StorageVersion};

pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
pub(crate) type BalanceOf<T> = <CurrencyOf<T> as Currency<AccountIdOf<T>>>::Balance;
pub(crate) type CurrencyOf<T> = <T as Config>::Currency;

/// The current storage version.
pub(crate) const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::GasPrice;
    use frame_support::{
        pallet_prelude::StorageMap,
        sp_runtime::{traits::CheckedSub, Saturating},
        traits::{ExistenceRequirement, Get, ReservableCurrency, WithdrawReasons},
        Identity,
    };
    use pallet_authorship::Pallet as Authorship;
    use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
    use scale_info::TypeInfo;

    // Funds pallet struct itself.
    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // Funds pallets config.
    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_authorship::Config {
        /// Balances management trait for gas/value migrations.
        type Currency: ReservableCurrency<AccountIdOf<Self>>;

        #[pallet::constant]
        /// Bank account address, that will keep all reserved funds.
        type BankAddress: Get<AccountIdOf<Self>>;
    }

    // Funds pallets error.
    #[pallet::error]
    pub enum Error<T> {
        /// Insufficient user balance.
        InsufficientBalance,
        /// Insufficient user's bank account gas balance.
        InsufficientGasBalance,
        /// Insufficient user's bank account gas balance.
        InsufficientValueBalance,
        /// Insufficient bank account balance.
        /// **Must be unreachable in Gear main protocol.**
        InsufficientBankBalance,
    }

    /// Type containing info of locked in special address funds of each account.
    #[derive(
        Debug,
        Default,
        Clone,
        Copy,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        MaxEncodedLen,
        Encode,
        Decode,
        TypeInfo,
    )]
    pub struct BankAccount<Balance> {
        /// Balance locked for gas purchase.
        pub gas: Balance,
        /// Balance locked for future transfer.
        pub value: Balance,
    }

    // Private storage that keeps account bank details.
    #[pallet::storage]
    pub type Bank<T> = StorageMap<_, Identity, AccountIdOf<T>, BankAccount<BalanceOf<T>>>;

    impl<T: Config> Pallet<T> {
        /// Transfers value from `account_id` to bank address.
        fn deposit(account_id: &AccountIdOf<T>, value: BalanceOf<T>) -> Result<(), Error<T>> {
            // Check on zero value is inside `pallet_balances` implementation.
            CurrencyOf::<T>::transfer(
                account_id,
                &T::BankAddress::get(),
                value,
                ExistenceRequirement::AllowDeath,
            )
            .map_err(|_| Error::<T>::InsufficientBalance)
        }

        /// Ensures that bank account is able to transfer requested value.
        fn ensure_bank_can_transfer(value: BalanceOf<T>) -> Result<(), Error<T>> {
            let bank_address = T::BankAddress::get();

            CurrencyOf::<T>::free_balance(&bank_address)
                .checked_sub(&value)
                .map_or(false, |new_balance| {
                    CurrencyOf::<T>::ensure_can_withdraw(
                        &bank_address,
                        value,
                        WithdrawReasons::TRANSFER,
                        new_balance,
                    )
                    .is_ok()
                })
                .then_some(())
                .ok_or(Error::<T>::InsufficientBankBalance)
        }

        /// Transfers value from bank address to `account_id`.
        fn withdraw(account_id: &AccountIdOf<T>, value: BalanceOf<T>) -> Result<(), Error<T>> {
            // Check on zero value is inside `pallet_balances` implementation.
            CurrencyOf::<T>::transfer(
                &T::BankAddress::get(),
                account_id,
                value,
                // We always require bank account to be alive.
                ExistenceRequirement::KeepAlive,
            )
            .map_err(|_| Error::<T>::InsufficientBankBalance)
        }

        /// Transfers value from bank address to current block author.
        fn reward_block_author(value: BalanceOf<T>) -> Result<(), Error<T>> {
            let block_author = Authorship::<T>::author()
                .unwrap_or_else(|| unreachable!("Failed to find block author!"));

            Self::withdraw(&block_author, value)
        }

        pub fn deposit_gas<P: GasPrice<Balance = BalanceOf<T>>>(
            account_id: &AccountIdOf<T>,
            amount: u64,
        ) -> Result<(), Error<T>> {
            let value = P::gas_price(amount);

            Self::deposit(account_id, value)?;

            Bank::<T>::mutate(account_id, |details| {
                let details = details.get_or_insert_with(Default::default);
                // There is no reason to return any errors on overflow, because
                // total value issuance is always lower than numeric MAX.
                //
                // Using saturating addition for code consistency.
                details.gas = details.gas.saturating_add(value);
            });

            Ok(())
        }

        fn withdraw_gas_no_transfer<P: GasPrice<Balance = BalanceOf<T>>>(
            account_id: &AccountIdOf<T>,
            amount: u64,
        ) -> Result<BalanceOf<T>, Error<T>> {
            // TODO: delete account on withdrawals and optimize if not exist.
            let value = P::gas_price(amount);

            Self::ensure_bank_can_transfer(value)?;

            Bank::<T>::mutate(account_id, |details| {
                let details = details.get_or_insert_with(Default::default);

                if let Some(new_gas) = details.gas.checked_sub(&value) {
                    details.gas = new_gas;
                    Ok(value)
                } else {
                    Err(Error::<T>::InsufficientGasBalance)
                }
            })
        }

        pub fn spend_gas<P: GasPrice<Balance = BalanceOf<T>>>(
            account_id: &AccountIdOf<T>,
            amount: u64,
        ) -> Result<(), Error<T>> {
            let value = Self::withdraw_gas_no_transfer::<P>(account_id, amount)?;

            Self::reward_block_author(value).unwrap_or_else(|_| unreachable!("qed above"));

            Ok(())
        }

        pub fn withdraw_gas<P: GasPrice<Balance = BalanceOf<T>>>(
            account_id: &AccountIdOf<T>,
            amount: u64,
        ) -> Result<(), Error<T>> {
            let value = Self::withdraw_gas_no_transfer::<P>(account_id, amount)?;

            Self::withdraw(account_id, value).unwrap_or_else(|_| unreachable!("qed above"));

            Ok(())
        }

        pub fn deposit_value(
            account_id: &AccountIdOf<T>,
            value: BalanceOf<T>,
        ) -> Result<(), Error<T>> {
            Self::deposit(account_id, value)?;

            Bank::<T>::mutate(account_id, |details| {
                let details = details.get_or_insert_with(Default::default);
                // There is no reason to return any errors on overflow, because
                // total value issuance is always lower than numeric MAX.
                //
                // Using saturating addition for code consistency.
                details.value = details.value.saturating_add(value);
            });

            Ok(())
        }

        fn withdraw_value_no_transfer(
            account_id: &AccountIdOf<T>,
            value: BalanceOf<T>,
        ) -> Result<(), Error<T>> {
            Self::ensure_bank_can_transfer(value)?;

            Bank::<T>::mutate(account_id, |details| {
                let details = details.get_or_insert_with(Default::default);

                if let Some(new_value) = details.value.checked_sub(&value) {
                    details.value = new_value;
                    Ok(())
                } else {
                    Err(Error::<T>::InsufficientValueBalance)
                }
            })
        }

        pub fn withdraw_value(
            account_id: &AccountIdOf<T>,
            value: BalanceOf<T>,
        ) -> Result<(), Error<T>> {
            Self::withdraw_value_no_transfer(account_id, value)?;

            Self::withdraw(account_id, value).unwrap_or_else(|_| unreachable!("qed above"));

            Ok(())
        }

        pub fn transfer_value<P: GasPrice<Balance = BalanceOf<T>>>(
            account_id: &AccountIdOf<T>,
            destination: &AccountIdOf<T>,
            value: BalanceOf<T>,
        ) -> Result<(), Error<T>> {
            Self::withdraw_value_no_transfer(account_id, value)?;

            Self::withdraw(destination, value).unwrap_or_else(|_| unreachable!("qed above"));

            Ok(())
        }

        pub fn get_account(account_id: &AccountIdOf<T>) -> BankAccount<BalanceOf<T>> {
            Bank::<T>::get(account_id).unwrap_or_default()
        }

        pub fn get_account_gas(account_id: &AccountIdOf<T>) -> BalanceOf<T> {
            Self::get_account(account_id).gas
        }

        pub fn get_account_value(account_id: &AccountIdOf<T>) -> BalanceOf<T> {
            Self::get_account(account_id).value
        }
    }
}

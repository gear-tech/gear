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

//! # Gear Bank Pallet.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub use pallet::*;

use frame_support::traits::{Currency, StorageVersion};

#[macro_export]
macro_rules! impl_config {
    ($runtime:ty) => {
        impl pallet_gear_bank::Config for $runtime {
            type Currency = Balances;
            type BankAddress = BankAddress;
            type GasMultiplier = GasMultiplier;
        }
    };
}

pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
pub(crate) type BalanceOf<T> = <CurrencyOf<T> as Currency<AccountIdOf<T>>>::Balance;
pub(crate) type CurrencyOf<T> = <T as Config>::Currency;
pub(crate) type GasMultiplier<T> = common::GasMultiplier<BalanceOf<T>, u64>;
pub(crate) type GasMultiplierOf<T> = <T as Config>::GasMultiplier;

/// The current storage version.
pub(crate) const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use core::ops::Add;
    use frame_support::{
        ensure,
        pallet_prelude::{StorageMap, StorageValue, ValueQuery},
        sp_runtime::{traits::CheckedSub, Saturating},
        traits::{ExistenceRequirement, Get, ReservableCurrency, WithdrawReasons},
        Identity,
    };
    use pallet_authorship::Pallet as Authorship;
    use parity_scale_codec::{Decode, Encode, EncodeLike, MaxEncodedLen};
    use scale_info::TypeInfo;
    use sp_runtime::traits::Zero;

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

        #[pallet::constant]
        /// Gas price converter.
        type GasMultiplier: Get<GasMultiplier<Self>>;
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
        /// Deposit of funds that will not keep bank account alive.
        /// **Must be unreachable in Gear main protocol.**
        InsufficientDeposit,
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

    impl<Balance: Add<Output = Balance> + Saturating> BankAccount<Balance> {
        pub fn total(self) -> Balance {
            self.gas.saturating_add(self.value)
        }
    }

    // Required by Zero trait impl.
    impl<Balance: Add<Output = Balance>> Add for BankAccount<Balance> {
        type Output = Self;

        fn add(self, rhs: Self) -> Self::Output {
            Self {
                gas: self.gas + rhs.gas,
                value: self.value + rhs.value,
            }
        }
    }

    impl<Balance: Zero> Zero for BankAccount<Balance> {
        fn zero() -> Self {
            Self {
                gas: Zero::zero(),
                value: Zero::zero(),
            }
        }

        fn is_zero(&self) -> bool {
            self.gas.is_zero() && self.value.is_zero()
        }
    }

    // Private storage that keeps account bank details.
    #[pallet::storage]
    pub type Bank<T> = StorageMap<_, Identity, AccountIdOf<T>, BankAccount<BalanceOf<T>>>;

    // Private storage that keeps amount of value that wasn't sent because owner is inexistent account.
    #[pallet::storage]
    pub type UnusedValue<T> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    impl<T: Config> Pallet<T> {
        /// Transfers value from `account_id` to bank address.
        fn deposit(
            account_id: &AccountIdOf<T>,
            value: BalanceOf<T>,
            keep_alive: bool,
        ) -> Result<(), Error<T>> {
            let bank_address = T::BankAddress::get();

            ensure!(
                CurrencyOf::<T>::free_balance(&bank_address).saturating_add(value)
                    >= CurrencyOf::<T>::minimum_balance(),
                Error::InsufficientDeposit
            );

            let existence_requirement = if keep_alive {
                ExistenceRequirement::KeepAlive
            } else {
                ExistenceRequirement::AllowDeath
            };

            // Check on zero value is inside `pallet_balances` implementation.
            CurrencyOf::<T>::transfer(account_id, &bank_address, value, existence_requirement)
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
            Self::ensure_bank_can_transfer(value)?;

            // The check is similar to one that used in transfer implementation.
            // It allows us define if we cannot transfer funds on early stage
            // to be able mean any transfer error as insufficient bank
            // account balance, because other conditions are checked
            // here and in other caller places.
            if CurrencyOf::<T>::free_balance(account_id).saturating_add(value)
                < CurrencyOf::<T>::minimum_balance()
            {
                UnusedValue::<T>::mutate(|unused_value| {
                    *unused_value = unused_value.saturating_add(value);
                });

                return Ok(());
            }

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

        pub fn deposit_gas(
            account_id: &AccountIdOf<T>,
            amount: u64,
            keep_alive: bool,
        ) -> Result<(), Error<T>> {
            if amount.is_zero() {
                return Ok(());
            }

            let value = GasMultiplierOf::<T>::get().gas_to_value(amount);

            Self::deposit(account_id, value, keep_alive)?;

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

        fn withdraw_gas_no_transfer(
            account_id: &AccountIdOf<T>,
            amount: u64,
            multiplier: GasMultiplier<T>,
        ) -> Result<BalanceOf<T>, Error<T>> {
            let value = multiplier.gas_to_value(amount);

            let gas_balance = Self::account_gas(account_id);

            ensure!(
                gas_balance.is_some() && gas_balance.expect("Checked before") >= value,
                Error::<T>::InsufficientGasBalance
            );

            Self::ensure_bank_can_transfer(value)?;

            Bank::<T>::mutate(account_id, |details_opt| {
                let details = details_opt.as_mut().expect("Checked above");

                // Insufficient case checked above.
                details.gas = details.gas.saturating_sub(value);

                if details.is_zero() {
                    *details_opt = None;
                }
            });

            Ok(value)
        }

        pub fn withdraw_gas(
            account_id: &AccountIdOf<T>,
            amount: u64,
            multiplier: GasMultiplier<T>,
        ) -> Result<(), Error<T>> {
            if amount.is_zero() {
                return Ok(());
            }

            let value = Self::withdraw_gas_no_transfer(account_id, amount, multiplier)?;

            // All the checks and internal values withdrawals performed in
            // `*_no_transfer` function above.
            //
            // This call does only currency trait final transfer.
            Self::withdraw(account_id, value).unwrap_or_else(|e| unreachable!("qed above: {e:?}"));

            Ok(())
        }

        pub fn spend_gas(
            account_id: &AccountIdOf<T>,
            amount: u64,
            multiplier: GasMultiplier<T>,
        ) -> Result<(), Error<T>> {
            let block_author = Authorship::<T>::author()
                .unwrap_or_else(|| unreachable!("Failed to find block author!"));

            Self::spend_gas_to(&block_author, account_id, amount, multiplier)
        }

        pub fn spend_gas_to(
            to: &AccountIdOf<T>,
            account_id: &AccountIdOf<T>,
            amount: u64,
            multiplier: GasMultiplier<T>,
        ) -> Result<(), Error<T>> {
            if amount.is_zero() {
                return Ok(());
            }

            let value = Self::withdraw_gas_no_transfer(account_id, amount, multiplier)?;

            // All the checks and internal values withdrawals performed in
            // `*_no_transfer` function above.
            //
            // This call does only currency trait final transfer.
            Self::withdraw(to, value).unwrap_or_else(|e| unreachable!("qed above: {e:?}"));

            Ok(())
        }

        pub fn deposit_value(
            account_id: &AccountIdOf<T>,
            value: BalanceOf<T>,
            keep_alive: bool,
        ) -> Result<(), Error<T>> {
            if value.is_zero() {
                return Ok(());
            }

            Self::deposit(account_id, value, keep_alive)?;

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
            let value_balance = Self::account_value(account_id);

            ensure!(
                value_balance.is_some() && value_balance.expect("Checked before") >= value,
                Error::<T>::InsufficientValueBalance
            );

            Self::ensure_bank_can_transfer(value)?;

            Bank::<T>::mutate(account_id, |details_opt| {
                let details = details_opt.as_mut().expect("Checked above");

                // Insufficient case checked above.
                details.value = details.value.saturating_sub(value);

                if details.is_zero() {
                    *details_opt = None;
                }
            });

            Ok(())
        }

        pub fn withdraw_value(
            account_id: &AccountIdOf<T>,
            value: BalanceOf<T>,
        ) -> Result<(), Error<T>> {
            if value.is_zero() {
                return Ok(());
            }

            Self::withdraw_value_no_transfer(account_id, value)?;

            // All the checks and internal values withdrawals performed in
            // `*_no_transfer` function above.
            //
            // This call does only currency trait final transfer.
            Self::withdraw(account_id, value).unwrap_or_else(|e| unreachable!("qed above: {e:?}"));

            Ok(())
        }

        // TODO: take care on this fn impl in case of bump ED (issue #3115).
        pub fn transfer_value(
            account_id: &AccountIdOf<T>,
            destination: &AccountIdOf<T>,
            value: BalanceOf<T>,
        ) -> Result<(), Error<T>> {
            if value.is_zero() {
                return Ok(());
            }

            Self::withdraw_value_no_transfer(account_id, value)?;

            // All the checks and internal values withdrawals performed in
            // `*_no_transfer` function above.
            //
            // This call does only currency trait final transfer.
            Self::withdraw(destination, value).unwrap_or_else(|e| unreachable!("qed above: {e:?}"));

            Ok(())
        }

        /// Getter for [`Bank<T>`](Bank)
        pub fn account<K: EncodeLike<AccountIdOf<T>>>(
            account_id: K,
        ) -> Option<BankAccount<BalanceOf<T>>> {
            Bank::<T>::get(account_id)
        }

        pub fn account_gas(account_id: &AccountIdOf<T>) -> Option<BalanceOf<T>> {
            Self::account(account_id).map(|v| v.gas)
        }

        pub fn account_value(account_id: &AccountIdOf<T>) -> Option<BalanceOf<T>> {
            Self::account(account_id).map(|v| v.value)
        }

        pub fn account_total(account_id: &AccountIdOf<T>) -> BalanceOf<T> {
            Self::account(account_id)
                .map(|v| v.total())
                .unwrap_or_default()
        }
    }
}

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

//! # Gear Bank Pallet.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::manual_inspect)]

extern crate alloc;

pub mod migrations;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use alloc::format;

pub use pallet::*;

use frame_support::{
    PalletId,
    pallet_prelude::BuildGenesisConfig,
    traits::{
        Currency, StorageVersion, fungible,
        tokens::{Fortitude, Preservation, Provenance},
    },
};

#[macro_export]
macro_rules! impl_config {
    ($( $tokens:tt )*) => {
        #[allow(dead_code)]
        type GearBankConfigTreasuryAddress = ();
        #[allow(dead_code)]
        type GearBankConfigTreasuryGasFeeShare = ();
        #[allow(dead_code)]
        type GearBankConfigTreasuryTxFeeShare = ();

        mod pallet_tests_gear_bank_config_impl {
            use super::*;

            $crate::impl_config_inner!($( $tokens )*);
        }
    };
}

#[macro_export]
macro_rules! impl_config_inner {
    ($runtime:ty$(,)?) => {
        impl pallet_gear_bank::Config for $runtime {
            type Currency = Balances;
            type PalletId = BankPalletId;
            type GasMultiplier = GasMultiplier;
            type TreasuryAddress = GearBankConfigTreasuryAddress;
            type TreasuryGasFeeShare = GearBankConfigTreasuryGasFeeShare;
            type TreasuryTxFeeShare = GearBankConfigTreasuryTxFeeShare;
        }
    };

    ($runtime:ty, TreasuryAddress = $treasury_address:ty $(, $( $rest:tt )*)?) => {
        type GearBankConfigTreasuryAddress = $treasury_address;

        $crate::impl_config_inner!($runtime, $($( $rest )*)?);
    };

    ($runtime:ty, TreasuryGasFeeShare = $treasury_gas_fee_share:ty $(, $( $rest:tt )*)?) => {
        type GearBankConfigTreasuryGasFeeShare = $treasury_gas_fee_share;

        $crate::impl_config_inner!($runtime, $($( $rest )*)?);
    };

    ($runtime:ty, TreasuryTxFeeShare = $treasury_tx_fee_share:ty $(, $( $rest:tt )*)?) => {
        type GearBankConfigTreasuryTxFeeShare = $treasury_tx_fee_share;

        $crate::impl_config_inner!($runtime, $($( $rest )*)?);
    };
}

pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
pub(crate) type BalanceOf<T> = <CurrencyOf<T> as Currency<AccountIdOf<T>>>::Balance;
pub(crate) type CurrencyOf<T> = <T as Config>::Currency;
pub(crate) type GasMultiplier<T> = common::GasMultiplier<BalanceOf<T>, u64>;
pub(crate) type GasMultiplierOf<T> = <T as Config>::GasMultiplier;

/// The current storage version.
pub(crate) const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use core::{marker::PhantomData, ops::Add};
    use frame_support::{
        Identity, ensure,
        pallet_prelude::{StorageMap, StorageValue, ValueQuery},
        sp_runtime::{Saturating, traits::AccountIdConversion},
        traits::{
            ExistenceRequirement, Get, Hooks, LockableCurrency, ReservableCurrency,
            tokens::DepositConsequence,
        },
        weights::Weight,
    };
    use frame_system::pallet_prelude::BlockNumberFor;
    use pallet_authorship::Pallet as Authorship;
    use parity_scale_codec::{Decode, Encode, EncodeLike, MaxEncodedLen};
    use scale_info::TypeInfo;
    use sp_runtime::{Percent, traits::Zero};

    // Funds pallet struct itself.
    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // Funds pallets config.
    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_authorship::Config {
        /// Balances management trait for gas/value migrations.
        type Currency: ReservableCurrency<AccountIdOf<Self>>
            + LockableCurrency<AccountIdOf<Self>>
            + fungible::Unbalanced<AccountIdOf<Self>, Balance = BalanceOf<Self>>;

        /// The gear bank's pallet id, used for deriving its sovereign account ID.
        #[pallet::constant]
        type PalletId: Get<PalletId>;

        /// Gas price converter.
        #[pallet::constant]
        type GasMultiplier: Get<GasMultiplier<Self>>;

        // TODO: consider making treasury related constant as storage items to support onchain updates #4058.
        #[pallet::constant]
        type TreasuryAddress: Get<AccountIdOf<Self>>;

        #[pallet::constant]
        type TreasuryGasFeeShare: Get<Percent>;

        #[pallet::constant]
        type TreasuryTxFeeShare: Get<Percent>;
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
        /// Overflow during funds transfer.
        /// **Must be unreachable in Gear main protocol.**
        Overflow,
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
    type Bank<T> = StorageMap<_, Identity, AccountIdOf<T>, BankAccount<BalanceOf<T>>>;

    /// The default account ID of the Gear Bank.
    pub struct DefaultBankAddress<T: Config>(PhantomData<T>);
    impl<T: Config> Get<AccountIdOf<T>> for DefaultBankAddress<T> {
        fn get() -> AccountIdOf<T> {
            Pallet::<T>::bank_address()
        }
    }

    // Private storage to hold the GearBank's AccountId.
    #[pallet::storage]
    pub type BankAddress<T> = StorageValue<_, AccountIdOf<T>, ValueQuery, DefaultBankAddress<T>>;

    // Private storage that keeps amount of value that wasn't sent because owner is inexistent account.
    #[pallet::storage]
    pub type UnusedValue<T> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    // Private storage that keeps registry of transfers to be performed at the end of the block.
    #[pallet::storage]
    type OnFinalizeTransfers<T> = StorageMap<_, Identity, AccountIdOf<T>, BalanceOf<T>>;

    // Private storage that represents sum of values in OnFinalizeTransfers.
    #[pallet::storage]
    pub(crate) type OnFinalizeValue<T> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        #[serde(skip)]
        pub _config: PhantomData<T>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            // Ensure the bank address is present in storage from the start.
            BankAddress::<T>::put(<Pallet<T>>::bank_address());
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// Start of the block.
        fn on_initialize(bn: BlockNumberFor<T>) -> Weight {
            if OnFinalizeTransfers::<T>::iter().next().is_some() {
                log::error!("Block #{bn:?} started with non-empty on-finalize transfers");
            }

            if !OnFinalizeValue::<T>::get().is_zero() {
                log::error!("Block #{bn:?} started with non-zero on-finalize value");
            }

            T::DbWeight::get().reads(2)
        }

        /// End of the block.
        fn on_finalize(bn: BlockNumberFor<T>) {
            // Take of on-finalize value should always be performed before
            // `withdraw`s, since `withdraw`s ensure bank balance,
            // that relies on that value "locked".
            let expected = OnFinalizeValue::<T>::take();

            let mut total = BalanceOf::<T>::zero();

            while let Some((account_id, value)) = OnFinalizeTransfers::<T>::drain().next() {
                total = total.saturating_add(value);

                if let Err(e) = Self::withdraw(&account_id, value) {
                    log::error!(
                        "Block #{bn:?} ended with unreachable error while performing on-finalize transfer to {account_id:?}: {e:?}"
                    );
                }
            }

            if total != expected {
                log::error!(
                    "Block #{bn:?} ended with unreachable error while performing cleaning of on-finalize value: \
                total tried to transfer is {total:?}, expected amount is {expected:?}"
                )
            }
        }
    }

    impl<T: Config> Pallet<T> {
        /// The account ID of the Gear Bank.
        ///
        /// This does some computation. Since GearBank account ID is used often,
        /// the value should better be cached in the pallet storage.
        pub fn bank_address() -> T::AccountId {
            T::PalletId::get().into_account_truncating()
        }

        /// Transfers value from `account_id` to bank address.
        fn deposit(
            account_id: &AccountIdOf<T>,
            value: BalanceOf<T>,
            keep_alive: bool,
        ) -> Result<(), Error<T>> {
            let bank_address = BankAddress::<T>::get();

            match <CurrencyOf<T> as fungible::Inspect<_>>::can_deposit(
                &bank_address,
                value,
                Provenance::Extant,
            ) {
                DepositConsequence::Success => (), // expected outcome
                DepositConsequence::BelowMinimum => return Err(Error::<T>::InsufficientDeposit),
                DepositConsequence::Overflow => return Err(Error::<T>::Overflow),
                // The rest is unreachable in Gear protocol and can be ignored.
                DepositConsequence::CannotCreate
                | DepositConsequence::UnknownAsset
                | DepositConsequence::Blocked => {
                    log::error!("Unexpected deposit consequence while depositing to bank address");
                }
            };

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
            let available_balance = <CurrencyOf<T> as fungible::Inspect<_>>::reducible_balance(
                &BankAddress::<T>::get(),
                Preservation::Expendable,
                Fortitude::Polite,
            )
            .saturating_sub(UnusedValue::<T>::get())
            .saturating_sub(OnFinalizeValue::<T>::get());

            (value <= available_balance)
                .then_some(())
                .ok_or(Error::<T>::InsufficientBankBalance)
        }

        /// Transfers value from bank address to `account_id`.
        fn withdraw(account_id: &AccountIdOf<T>, value: BalanceOf<T>) -> Result<(), Error<T>> {
            Self::ensure_bank_can_transfer(value)?;

            // Since funds are not being minted here but transferred, the only error we can
            // possibly observe is the `TokenError::BelowMinimum` one (no overflow whatsoever).
            // It means we can check for the outcome being just any error and be sure it is
            // that the recipient account would die as a result of this transfer.
            if <CurrencyOf<T> as fungible::Inspect<_>>::can_deposit(
                account_id,
                value,
                Provenance::Extant,
            )
            .into_result()
            .is_err()
            {
                UnusedValue::<T>::mutate(|unused_value| {
                    *unused_value = unused_value.saturating_add(value);
                });

                return Ok(());
            }

            // Check on zero value is inside `pallet_balances` implementation.
            CurrencyOf::<T>::transfer(
                &BankAddress::<T>::get(),
                account_id,
                value,
                // We always require bank account to be alive.
                ExistenceRequirement::KeepAlive,
            )
            .map_err(|_| Error::<T>::InsufficientBankBalance)
        }

        /// Transfers value from bank address to `account_id` on block finalize.
        fn withdraw_on_finalize(
            account_id: &AccountIdOf<T>,
            value: BalanceOf<T>,
        ) -> Result<(), Error<T>> {
            if value.is_zero() {
                return Ok(());
            };

            Self::ensure_bank_can_transfer(value)?;

            OnFinalizeValue::<T>::mutate(|v| *v = v.saturating_add(value));
            OnFinalizeTransfers::<T>::mutate(account_id, |v| {
                let inner = v.get_or_insert(Zero::zero());
                *inner = inner.saturating_add(value);
            });

            Ok(())
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
            Self::withdraw(account_id, value).unwrap_or_else(|e| {
                let receiver_balance = Self::reducible_balance(account_id);
                let (bank_balance, unused_value, on_finalize_value) = Self::bank_balance_full_data();

                let err_msg = format!(
                    "pallet_gear_bank::withdraw_gas: withdraw failed. \
                    Receiver - {account_id:?}, amount - {amount}, receiver reducible balance - {receiver_balance:?}, \
                    bank reducible balance - {bank_balance:?}, unused value - {unused_value:?}, on finalize value - {on_finalize_value:?}. \
                    Got error - {e:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            });

            Ok(())
        }

        pub fn spend_gas(
            account_id: &AccountIdOf<T>,
            amount: u64,
            multiplier: GasMultiplier<T>,
        ) -> Result<(), Error<T>> {
            let block_author = Authorship::<T>::author().unwrap_or_else(|| {
                let err_msg = "pallet_gear_bank::spend_gas: Failed to find block author";

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            });

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

            let treasury = T::TreasuryAddress::get();
            let treasury_share = T::TreasuryGasFeeShare::get() * value;

            Self::withdraw_on_finalize(&treasury, treasury_share).unwrap_or_else(|e| {
                let treasury_balance = Self::reducible_balance(&treasury);
                let (bank_balance, unused_value, on_finalize_value) = Self::bank_balance_full_data();

                let err_msg = format!(
                    "pallet_gear_bank::spend_gas_to: withdraw to TREASURY on finalize failed. \
                    Spending gas from - {account_id:?}, value - {treasury_share:?}, TREASURY reducible balance {treasury_balance:?} \
                    bank reducible balance - {bank_balance:?}, unused value - {unused_value:?}, on finalize value - {on_finalize_value:?}. \
                    Got error - {e:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            });

            let value = value - treasury_share;
            Self::withdraw_on_finalize(to, value).unwrap_or_else(|e| {
                let to_balance = Self::reducible_balance(to);
                let (bank_balance, unused_value, on_finalize_value) = Self::bank_balance_full_data();

                let err_msg = format!(
                    "pallet_gear_bank::spend_gas_to: withdraw on finalize failed. \
                    Spending gas from - {account_id:?}, to - {to:?}, value - {value:?}, receiver reducible balance {to_balance:?} \
                    bank reducible balance - {bank_balance:?}, unused value - {unused_value:?}, on finalize value - {on_finalize_value:?}. \
                    Got error - {e:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            });

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
            Self::withdraw(account_id, value).unwrap_or_else(|e| {
                let receiver_balance = Self::reducible_balance(account_id);
                let (bank_balance, unused_value, on_finalize_value) = Self::bank_balance_full_data();

                let err_msg = format!(
                    "pallet_gear_bank::withdraw_value: withdraw failed. \
                    Receiver - {account_id:?}, value - {value:?}, receiver reducible balance - {receiver_balance:?}, \
                    bank reducible balance - {bank_balance:?}, unused value - {unused_value:?},
                    on finalize value - {on_finalize_value:?}. \
                    Got error - {e:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            });

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
            Self::withdraw(destination, value).unwrap_or_else(|e| {
                let receiver_balance = Self::reducible_balance(destination);
                let (bank_balance, unused_value, on_finalize_value) = Self::bank_balance_full_data();

                let err_msg = format!(
                    "pallet_gear_bank::transfer_value: withdraw failed. \
                    Sender - {account_id:?}, receiver - {destination:?}, value - {value:?},
                    receiver reducible balance - {receiver_balance:?}, bank reducible balance - {bank_balance:?}, \
                    unused value - {unused_value:?}, on finalize value - {on_finalize_value:?} \
                    Got error - {e:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            });

            Ok(())
        }

        pub fn transfer_locked_value(
            account_id: &AccountIdOf<T>,
            destination: &AccountIdOf<T>,
            value: BalanceOf<T>,
        ) -> Result<(), Error<T>> {
            if value.is_zero() {
                return Ok(());
            }

            Self::withdraw_value_no_transfer(account_id, value)?;

            Bank::<T>::mutate(destination, |details| {
                let details = details.get_or_insert_with(Default::default);
                // There is no reason to return any errors on overflow, because
                // total value issuance is always lower than numeric MAX.
                //
                // Using saturating addition for code consistency.
                details.value = details.value.saturating_add(value);
            });

            Ok(())
        }

        /// Getter for `Bank<T>`
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

        fn bank_balance_full_data() -> (BalanceOf<T>, BalanceOf<T>, BalanceOf<T>) {
            (
                Self::reducible_balance(&BankAddress::<T>::get()),
                UnusedValue::<T>::get(),
                OnFinalizeValue::<T>::get(),
            )
        }

        fn reducible_balance(account_id: &AccountIdOf<T>) -> BalanceOf<T> {
            <CurrencyOf<T> as fungible::Inspect<_>>::reducible_balance(
                account_id,
                Preservation::Expendable,
                Fortitude::Polite,
            )
        }
    }
}

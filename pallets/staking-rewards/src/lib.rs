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

//! # Gear Staking Rewards Pallet
//!
//! The Staking Rewards pallet provides a pool that holds funds used to offset validators
//! rewards.
//!
//! - [`Config`]
//! - [`Call`]
//!
//! ## Overview
//!
//! The Staking Rewards pallet provides a pool that allows to postpone the inflationary impact
//! of the validators rewards minted out of thin air at the end of every era until the pool is
//! completely depleted after a certain period of time (approx. 2 years).
//! Thereby the nominal base token inflation stays around zero. Instead, the so-called
//! "stakeable tokens" amount is increased by the delta minted due to the inflation.
//! After the pools is depleted the inflation will start affecting the base token total issuance
//! in a usual Substrate fashion.
//!
//! The pallet implements the `pallet_staking::EraPayout<Balance>` trait
//!

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::items_after_test_module)]
#![allow(clippy::manual_inspect)]

pub mod extension;
pub mod weights;

mod inflation;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use frame_support::{
    PalletId,
    traits::{
        Contains, Currency, ExistenceRequirement, Get, Imbalance, OnUnbalanced, WithdrawReasons,
        fungible,
    },
    weights::Weight,
};
use pallet_staking::{ActiveEraInfo, EraPayout};
use parity_scale_codec::{Decode, Encode};
use sp_runtime::{
    PerThing, Perquintill,
    traits::{AccountIdConversion, Saturating, StaticLookup, UniqueSaturatedInto},
};
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

pub use extension::StakingBlackList;
pub use inflation::compute_total_payout;
pub use pallet::*;
pub use scale_info::TypeInfo;
pub use weights::WeightInfo;

pub type BalanceOf<T> = <T as pallet_staking::Config>::CurrencyBalance;
pub type CurrencyOf<T> = <T as pallet_staking::Config>::Currency;
pub type PositiveImbalanceOf<T> = <<T as pallet_staking::Config>::Currency as Currency<
    <T as frame_system::Config>::AccountId,
>>::PositiveImbalance;
pub type NegativeImbalanceOf<T> = <<T as pallet_staking::Config>::Currency as Currency<
    <T as frame_system::Config>::AccountId,
>>::NegativeImbalance;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;
pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

/// Token economics related details.
#[derive(Clone, Decode, Encode, Eq, PartialEq, TypeInfo)]
#[cfg_attr(feature = "std", derive(Debug, serde::Deserialize, serde::Serialize))]
pub struct InflationInfo {
    /// Inflation
    pub inflation: Perquintill,
    /// ROI
    pub roi: Perquintill,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use core::cmp::Ordering;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    /// The current storage version.
    const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

    pub struct RentPoolId<T: Config>(PhantomData<T>);

    impl<T: Config> Get<Option<AccountIdOf<T>>> for RentPoolId<T> {
        fn get() -> Option<AccountIdOf<T>> {
            Some(Pallet::<T>::rent_pool_account_id())
        }
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_staking::Config {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// RuntimeCall filter that matches the `Staking::bond()` call
        type BondCallFilter: Contains<<Self as frame_system::Config>::RuntimeCall>;

        /// Filter that determines whether a provided account has some property
        type AccountFilter: Contains<Self::AccountId>;

        /// The staking rewards' pallet id, used for deriving its sovereign account ID.
        #[pallet::constant]
        type PalletId: Get<PalletId>;

        /// Origin for adding funds to the pool.
        type RefillOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Origin for withdrawing funds from the pool.
        type WithdrawOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Milliseconds per year to calculate inflation.
        #[pallet::constant]
        type MillisecondsPerYear: Get<u64>;

        /// Minimum annual inflation.
        #[pallet::constant]
        type MinInflation: Get<Perquintill>;

        /// ROI cap.
        #[pallet::constant]
        type MaxROI: Get<Perquintill>;

        /// Exponential decay (fall-off) parameter.
        #[pallet::constant]
        type Falloff: Get<Perquintill>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;
    }

    /// Target inflation (at ideal stake)
    #[pallet::storage]
    pub(crate) type TargetInflation<T> = StorageValue<_, Perquintill, ValueQuery>;

    /// Ideal staking ratio
    #[pallet::storage]
    pub(crate) type IdealStakingRatio<T> = StorageValue<_, Perquintill, ValueQuery>;

    /// The current share of issued tokens that cannot be staked (e.g. being vested)
    /// This value is guaranteed to remain unchanged for the first year until vesting kicks in.
    /// Subsequently, the non-stakeable share should be calculated based on the vesting balances.
    #[pallet::storage]
    pub type NonStakeableShare<T> = StorageValue<_, Perquintill, ValueQuery>;

    /// List of accounts whose locked balance (due to incomplete vesting) should be excluded from
    /// the total stakeable quantity.
    /// During the 1st year the non-stakeable amount is accounted for as a fixed fraction of TTS.
    #[pallet::storage]
    pub type FilteredAccounts<T: Config> = StorageValue<_, BTreeSet<T::AccountId>, ValueQuery>;

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        pub pool_balance: BalanceOf<T>,
        pub non_stakeable: Perquintill,
        pub ideal_stake: Perquintill,
        pub target_inflation: Perquintill,
        pub filtered_accounts: Vec<T::AccountId>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            // Create StakingRewards account
            let account_id = <Pallet<T>>::account_id();
            let amount = self
                .pool_balance
                .saturating_add(T::Currency::minimum_balance());
            if T::Currency::free_balance(&account_id) < amount {
                // Set the staking rewards pool account balance to the initial value.
                // Dropping the resulting imbalance as the funds are minted out of thin air.
                let _ = T::Currency::make_free_balance_be(&account_id, amount);
            }

            // create account for the rent pool
            let _ = T::Currency::make_free_balance_be(
                &Pallet::<T>::rent_pool_account_id(),
                T::Currency::minimum_balance(),
            );

            TargetInflation::<T>::put(self.target_inflation);
            IdealStakingRatio::<T>::put(self.ideal_stake);
            NonStakeableShare::<T>::put(self.non_stakeable);
            FilteredAccounts::<T>::put(
                self.filtered_accounts
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>(),
            );
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Deposited to the pool.
        Deposited { amount: BalanceOf<T> },
        /// Transferred from the pool to an external account.
        Withdrawn { amount: BalanceOf<T> },
        /// Burned from the pool.
        Burned { amount: BalanceOf<T> },
        /// Minted to the pool.
        Minted { amount: BalanceOf<T> },
    }

    /// Error for the staking rewards pallet.
    #[pallet::error]
    pub enum Error<T> {
        /// Pool not replenished due to error.
        FailureToRefillPool,
        /// Failure to withdraw funds from the rewards pool.
        FailureToWithdrawFromPool,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
            Weight::zero()
        }
    }

    impl<T: Config> Pallet<T> {
        /// Getter for [`FilteredAccounts<T>`](FilteredAccounts)
        pub fn filtered_accounts() -> BTreeSet<T::AccountId> {
            FilteredAccounts::<T>::get()
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::refill())]
        pub fn refill(origin: OriginFor<T>, value: BalanceOf<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            <T as pallet_staking::Config>::Currency::transfer(
                &who,
                &Self::account_id(),
                value,
                ExistenceRequirement::AllowDeath,
            )
            .map_err(|e| {
                log::error!("Failed to replenish the staking rewards pool: {e:?}");
                Error::<T>::FailureToRefillPool
            })?;
            Self::deposit_event(Event::Deposited { amount: value });

            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::force_refill())]
        pub fn force_refill(
            origin: OriginFor<T>,
            from: AccountIdLookupOf<T>,
            value: BalanceOf<T>,
        ) -> DispatchResult {
            T::RefillOrigin::ensure_origin(origin)?;
            let from = T::Lookup::lookup(from)?;
            <T as pallet_staking::Config>::Currency::transfer(
                &from,
                &Self::account_id(),
                value,
                ExistenceRequirement::AllowDeath,
            )
            .map_err(|e| {
                log::error!("Failed to replenish the staking rewards pool: {e:?}");
                Error::<T>::FailureToRefillPool
            })?;
            Self::deposit_event(Event::Deposited { amount: value });

            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::withdraw())]
        pub fn withdraw(
            origin: OriginFor<T>,
            to: AccountIdLookupOf<T>,
            value: BalanceOf<T>,
        ) -> DispatchResult {
            T::WithdrawOrigin::ensure_origin(origin)?;
            let to = T::Lookup::lookup(to)?;
            <T as pallet_staking::Config>::Currency::transfer(
                &Self::account_id(),
                &to,
                value,
                ExistenceRequirement::AllowDeath,
            )
            .map_err(|e| {
                log::error!("Failed to withdraw funds from the staking rewards pool: {e:?}");
                Error::<T>::FailureToWithdrawFromPool
            })?;
            Self::deposit_event(Event::Withdrawn { amount: value });

            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::align_supply())]
        pub fn align_supply(origin: OriginFor<T>, target: BalanceOf<T>) -> DispatchResult {
            ensure_root(origin)?;

            let issuance = T::Currency::total_issuance();

            match target.cmp(&issuance) {
                Ordering::Greater => {
                    OffsetPool::<T>::on_nonzero_unbalanced(T::Currency::issue(target - issuance));
                }
                Ordering::Less => {
                    Self::on_nonzero_unbalanced(T::Currency::burn(issuance - target));
                }
                _ => {}
            };

            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        /// The account ID of the staking rewards pool.
        ///
        /// This actually does computation. If used often, the value should better be cached
        /// so that this function is only called once.
        pub fn account_id() -> T::AccountId {
            T::PalletId::get().into_account_truncating()
        }

        /// Return the amount in the staking rewards pool.
        // The existential deposit is not a part of the pool so rewards account never gets deleted.
        pub fn pool() -> BalanceOf<T> {
            T::Currency::free_balance(&Self::account_id())
                // Must never be less than 0 but better be safe.
                .saturating_sub(T::Currency::minimum_balance())
        }

        /// Return the current total stakeable tokens amount.
        ///
        /// This value is not calculated but rather updated manually in line with tokenomics model.
        pub fn total_stakeable_tokens() -> BalanceOf<T> {
            // Should never be 0 but in theory could
            (NonStakeableShare::<T>::get().left_from_one() * T::Currency::total_issuance())
                .saturating_sub(Self::pool())
        }

        /// Calculate actual inflation and ROI parameters.
        pub fn inflation_info() -> InflationInfo {
            let total_staked = pallet_staking::Pallet::<T>::eras_total_stake(
                pallet_staking::Pallet::<T>::current_era().unwrap_or(0),
            );
            let total_issuance = T::Currency::total_issuance();

            let (payout, _) = inflation::compute_total_payout(
                total_staked,
                Self::total_stakeable_tokens(),
                total_issuance,
                IdealStakingRatio::<T>::get(),
                T::MinInflation::get(),
                TargetInflation::<T>::get(),
                T::Falloff::get(),
                T::MaxROI::get(),
                Perquintill::one(),
            );

            let inflation = Perquintill::from_rational(payout, total_issuance);
            let roi = Perquintill::from_rational(payout, total_staked);

            InflationInfo { inflation, roi }
        }

        /// The account ID of the rent rewards pool.
        pub fn rent_pool_account_id() -> T::AccountId {
            use sp_runtime::traits::TrailingZeroInput;

            let entropy =
                (T::PalletId::get(), b"gear rent pool").using_encoded(sp_io::hashing::blake2_256);
            Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
                .expect("infinite length input; no invalid inputs for type; qed")
        }

        /// Return the amount in the rent pool.
        // The existential deposit is not a part of the pool so the account never gets deleted.
        pub fn rent_pool_balance() -> BalanceOf<T> {
            T::Currency::free_balance(&Self::rent_pool_account_id())
                // Must never be less than 0 but better be safe.
                .saturating_sub(T::Currency::minimum_balance())
        }
    }
}

// TODO: consider to optimize the process #3729
fn pay_rent_rewards_out<T: Config>(maybe_active_era_info: Option<ActiveEraInfo>) {
    let Some(active_era_info) = maybe_active_era_info else {
        return;
    };

    let reward_points = pallet_staking::Pallet::<T>::eras_reward_points(active_era_info.index);
    let total = u128::from(reward_points.total);
    if total == 0 {
        return;
    }

    let funds: u128 = pallet::Pallet::<T>::rent_pool_balance().unique_saturated_into();
    for (account_id, points) in reward_points.individual {
        let payout = funds.saturating_mul(u128::from(points)) / total;
        if payout > 0 {
            CurrencyOf::<T>::transfer(
                &pallet::Pallet::<T>::rent_pool_account_id(),
                &account_id,
                payout.unique_saturated_into(),
                ExistenceRequirement::KeepAlive,
            )
            .unwrap_or_else(|e| log::error!("Failed to transfer rent reward: {e:?}; account_id = {account_id:#?}, points = {points}, payout = {payout}"));
        }
    }
}

impl<T: Config> EraPayout<BalanceOf<T>> for Pallet<T> {
    fn era_payout(
        total_staked: BalanceOf<T>,
        total_issuance: BalanceOf<T>,
        era_duration_millis: u64,
    ) -> (BalanceOf<T>, BalanceOf<T>) {
        pay_rent_rewards_out::<T>(pallet_staking::Pallet::<T>::active_era());

        let period_fraction =
            Perquintill::from_rational(era_duration_millis, T::MillisecondsPerYear::get());
        inflation::compute_total_payout(
            total_staked,
            Self::total_stakeable_tokens(),
            total_issuance,
            IdealStakingRatio::<T>::get(),
            T::MinInflation::get(),
            TargetInflation::<T>::get(),
            T::Falloff::get(),
            T::MaxROI::get(),
            period_fraction,
        )
    }
}

/// Balance out excessive total supply whenever new tokens are minted through
/// burning the equivalent amount from the inflation offset pool
impl<T: Config> OnUnbalanced<PositiveImbalanceOf<T>> for Pallet<T> {
    fn on_nonzero_unbalanced(minted: PositiveImbalanceOf<T>) {
        let amount = minted.peek();

        if let Ok(burned) = T::Currency::withdraw(
            &Self::account_id(),
            amount,
            WithdrawReasons::TRANSFER,
            ExistenceRequirement::KeepAlive,
        ) {
            // Offsetting rewards against rewards pool until the latter is not depleted.
            // After that the positive imbalance is dropped adding up to the total supply.
            let _ = minted.offset(burned);

            Self::deposit_event(Event::Burned { amount });
        } else {
            log::warn!(
                "Staking rewards pool has insufficient balance to burn minted rewards. \
                The currency total supply may grow."
            );
        };
    }
}

/// Funnel the funds-to-burn into the inflation offset pool to maintain the total supply
pub struct OffsetPool<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> OnUnbalanced<NegativeImbalanceOf<T>> for OffsetPool<T> {
    fn on_nonzero_unbalanced(amount: NegativeImbalanceOf<T>) {
        let numeric_amount = amount.peek();

        // Should resolve into existing but resolving with creation is a safer bet anyway
        T::Currency::resolve_creating(&Pallet::<T>::account_id(), amount);

        Pallet::deposit_event(Event::<T>::Minted {
            amount: numeric_amount,
        });
    }
}

// DustRemoval handler
pub struct OffsetPoolDust<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> OnUnbalanced<fungible::Credit<T::AccountId, CurrencyOf<T>>> for OffsetPoolDust<T>
where
    CurrencyOf<T>: fungible::Balanced<T::AccountId, Balance = BalanceOf<T>>,
    BalanceOf<T>: Send + Sync + 'static,
{
    fn on_nonzero_unbalanced(amount: fungible::Credit<T::AccountId, CurrencyOf<T>>) {
        let numeric_amount = amount.peek();

        let result =
            <CurrencyOf<T> as fungible::Balanced<_>>::resolve(&Pallet::<T>::account_id(), amount);
        match result {
            Ok(()) => Pallet::deposit_event(Event::<T>::Minted {
                amount: numeric_amount,
            }),
            Err(amount) => log::error!("Balanced::resolve() err: {:?}", amount.peek()),
        }
    }
}

/// A type to be plugged into the Staking pallet as the `RewardRemainder` associated type.
///
/// A wrapper around the final `RewardRemainder` destination that burns from the inflation offset
/// pool the equivalent of the provided `NegativeImbalance` value in order to balance out what has
/// been minted as a part of the staking rewards for the era but not yet attributed to any account.
/// It is assumed that the subsequent `OnUnbalanced` handler (e.g. Treasury) would `resolve` the
/// imbalance and not drop it - otherwise the the total supply will decrease.
pub struct RewardProxy<T, U>(sp_std::marker::PhantomData<(T, U)>);

impl<T: Config, U> OnUnbalanced<NegativeImbalanceOf<T>> for RewardProxy<T, U>
where
    U: OnUnbalanced<NegativeImbalanceOf<T>>,
{
    fn on_nonzero_unbalanced(amount: NegativeImbalanceOf<T>) {
        let numeric_amount = amount.peek();

        // Try to burn the respective amount from the staking rewards pool and drop
        // the output to offset the total issuance increase which should have taken place
        // somewhere upstream when the incoming negative imbalance `amount` was created
        let _ = T::Currency::withdraw(
            &Pallet::<T>::account_id(),
            numeric_amount,
            WithdrawReasons::TRANSFER,
            ExistenceRequirement::KeepAlive,
        );

        U::on_unbalanced(amount);
    }
}

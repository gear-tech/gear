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

//! Staking builtin actor implementation

use super::*;
use common::Origin;
use core::marker::PhantomData;
use gbuiltin_staking::*;
use pallet_staking::{Config as StakingConfig, NominationsQuota, RewardDestination};
use parity_scale_codec::Decode;
use sp_runtime::traits::{StaticLookup, UniqueSaturatedInto};

pub struct Actor<T: Config + StakingConfig>(PhantomData<T>);

impl<T: Config + StakingConfig> Actor<T>
where
    T::AccountId: Origin,
    CallOf<T>: From<pallet_staking::Call<T>>,
{
    fn cast(request: Request) -> CallOf<T> {
        match request {
            Request::Bond { value, payee } => {
                let payee = match payee {
                    RewardAccount::Staked => RewardDestination::Staked,
                    RewardAccount::Program => RewardDestination::Stash,
                    RewardAccount::Custom(account_id) => {
                        RewardDestination::Account(account_id.cast())
                    }
                    RewardAccount::None => RewardDestination::None,
                };
                pallet_staking::Call::<T>::bond {
                    value: value.unique_saturated_into(),
                    payee,
                }
                .into()
            }
            Request::BondExtra { value } => pallet_staking::Call::<T>::bond_extra {
                max_additional: value.unique_saturated_into(),
            }
            .into(),
            Request::Unbond { value } => pallet_staking::Call::<T>::unbond {
                value: value.unique_saturated_into(),
            }
            .into(),
            Request::WithdrawUnbonded { num_slashing_spans } => {
                pallet_staking::Call::<T>::withdraw_unbonded { num_slashing_spans }.into()
            }
            Request::Nominate { targets } => pallet_staking::Call::<T>::nominate {
                targets: targets
                    .into_iter()
                    .map(|account_id| T::Lookup::unlookup(account_id.cast()))
                    .collect(),
            }
            .into(),
            Request::Chill => pallet_staking::Call::<T>::chill {}.into(),
            Request::PayoutStakers {
                validator_stash,
                era,
            } => {
                let stash_id = validator_stash.cast();
                pallet_staking::Call::<T>::payout_stakers {
                    validator_stash: stash_id,
                    era,
                }
                .into()
            }
            Request::Rebond { value } => pallet_staking::Call::<T>::rebond {
                value: value.unique_saturated_into(),
            }
            .into(),
            Request::SetPayee { payee } => {
                let payee = match payee {
                    RewardAccount::Staked => RewardDestination::Staked,
                    RewardAccount::Program => RewardDestination::Stash,
                    RewardAccount::Custom(account_id) => {
                        RewardDestination::Account(account_id.cast())
                    }
                    RewardAccount::None => RewardDestination::None,
                };
                pallet_staking::Call::<T>::set_payee { payee }.into()
            }
        }
    }
}

impl<T: Config + StakingConfig> BuiltinActor for Actor<T>
where
    T::AccountId: Origin,
    CallOf<T>: From<pallet_staking::Call<T>>,
{
    fn handle(
        dispatch: &StoredDispatch,
        context: &mut BuiltinContext,
    ) -> Result<BuiltinHandleResult, BuiltinActorError> {
        let message = dispatch.message();
        let origin = dispatch.source();
        let mut payload = message.payload_bytes();

        // Rule out payloads that exceed the largest reasonable size.
        // The longest payload corresponds to the `Request::Nominate` variant and is capped at
        // MaxNominatorTargets * 32 bytes + 2 bytes for the length prefix.
        // Adding extra 10% we can safely assume that any payload exceeding this size is invalid.
        let max_payload_size =
            (<T as StakingConfig>::NominationsQuota::get_quota(Default::default()) as usize * 32
                + 2)
                * 11
                / 10;
        if payload.len() > max_payload_size {
            return Err(BuiltinActorError::DecodingError);
        }

        // Decode the message payload to derive the desired action
        let request =
            Request::decode(&mut payload).map_err(|_| BuiltinActorError::DecodingError)?;

        // Handle staking requests
        let call = Self::cast(request);

        Ok(BuiltinHandleResult {
            payload: Pallet::<T>::dispatch_call(origin, call, context)
                .map(|_| Default::default())?,
            used_value: 0,
        })
    }

    fn max_gas() -> u64 {
        Default::default()
    }
}

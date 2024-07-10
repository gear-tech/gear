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

//! Staking builtin actor implementation

use super::*;
use common::Origin;
use core::marker::PhantomData;
use frame_support::dispatch::{extract_actual_weight, GetDispatchInfo};
use gbuiltin_staking::*;
use pallet_staking::{Config as StakingConfig, NominationsQuota, RewardDestination};
use parity_scale_codec::Decode;
use sp_runtime::traits::{Dispatchable, StaticLookup, UniqueSaturatedInto};

type CallOf<T> = <T as Config>::RuntimeCall;

pub struct Actor<T: Config + StakingConfig>(PhantomData<T>);

impl<T: Config + StakingConfig> Actor<T>
where
    T::AccountId: Origin,
    CallOf<T>: From<pallet_staking::Call<T>>,
{
    fn dispatch_call(
        origin: T::AccountId,
        call: CallOf<T>,
        gas_limit: u64,
    ) -> (Result<(), BuiltinActorError>, u64) {
        let call_info = call.get_dispatch_info();

        // Necessary upfront gas sufficiency check
        if gas_limit < call_info.weight.ref_time() {
            return (Err(BuiltinActorError::InsufficientGas), 0_u64);
        }

        // Execute call
        let res = call.dispatch(frame_system::RawOrigin::Signed(origin).into());
        let actual_gas = extract_actual_weight(&res, &call_info).ref_time();
        match res {
            Ok(_post_info) => {
                log::debug!(
                    target: LOG_TARGET,
                    "Call dispatched successfully",
                );
                (Ok(()), actual_gas)
            }
            Err(e) => {
                log::debug!(target: LOG_TARGET, "Error dispatching call: {:?}", e);
                (
                    Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                        e.into(),
                    ))),
                    actual_gas,
                )
            }
        }
    }

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
    const ID: u64 = 2;

    type Error = BuiltinActorError;

    fn handle(dispatch: &StoredDispatch, gas_limit: u64) -> (Result<Payload, Self::Error>, u64) {
        let message = dispatch.message();
        let origin: T::AccountId = dispatch.source().cast();
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
            return (Err(BuiltinActorError::PayloadTooLarge), 0);
        }

        // Decode the message payload to derive the desired action
        let Ok(request) = Request::decode(&mut payload) else {
            return (Err(BuiltinActorError::DecodingError), 0);
        };

        // Handle staking requests
        let call = Self::cast(request);
        let (result, gas_spent) = Self::dispatch_call(origin, call, gas_limit);

        (result.map(|_| Default::default()), gas_spent)
    }
}

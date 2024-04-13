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
use pallet_staking::RewardDestination;
use parity_scale_codec::Decode;
use sp_runtime::traits::{Dispatchable, StaticLookup, UniqueSaturatedInto};

pub struct Actor<T: Config>(PhantomData<T>);

impl<T: Config> Actor<T>
where
    T::AccountId: Origin,
{
    fn dispatch_call(
        origin: T::AccountId,
        call: <T as Config>::RuntimeCall,
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
}

impl<T: Config> BuiltinActor for Actor<T>
where
    T::AccountId: Origin,
{
    const ID: u64 = 2;

    type Error = BuiltinActorError;

    fn handle(dispatch: &StoredDispatch, gas_limit: u64) -> (Result<Payload, Self::Error>, u64) {
        let message = dispatch.message();
        let origin: T::AccountId = dispatch.source().cast();
        let mut payload = message.payload_bytes();

        let decoding_cost =
            <T as Config>::WeightInfo::staking_estimate_decode(payload.len() as u32).ref_time();

        // Decode the message payload to derive the desired action
        let (result, gas_spent) = match Request::decode(&mut payload) {
            Ok(request) => {
                // Handle staking requests
                let gas_limit = gas_limit.saturating_sub(decoding_cost);
                let call = match request {
                    Request::Bond { value, payee } => {
                        let payee = match payee {
                            RewardAccount::Staked => RewardDestination::Staked,
                            RewardAccount::Program => RewardDestination::Stash,
                            RewardAccount::Custom(account_id) => {
                                let dest = ProgramId::try_from(&account_id[..])
                                    .unwrap_or_else(|_e| unreachable!("32 bytes type; qed"))
                                    .cast();
                                RewardDestination::Account(dest)
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
                            .map(|account_id| {
                                let origin = ProgramId::try_from(&account_id[..])
                                    .unwrap_or_else(|_e| unreachable!("32 bytes type; qed"))
                                    .cast();
                                T::Lookup::unlookup(origin)
                            })
                            .collect(),
                    }
                    .into(),
                    Request::Chill => pallet_staking::Call::<T>::chill {}.into(),
                    Request::PayoutStakers {
                        validator_stash,
                        era,
                    } => {
                        let stash_id = ProgramId::try_from(&validator_stash[..])
                            .unwrap_or_else(|_e| unreachable!("32 bytes type; qed"))
                            .cast();
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
                                let dest = ProgramId::try_from(&account_id[..])
                                    .unwrap_or_else(|_e| unreachable!("32 bytes type; qed"))
                                    .cast();
                                RewardDestination::Account(dest)
                            }
                            RewardAccount::None => RewardDestination::None,
                        };
                        pallet_staking::Call::<T>::set_payee { payee }.into()
                    }
                };
                Self::dispatch_call(origin, call, gas_limit)
            }
            _ => (Err(BuiltinActorError::DecodingError), Default::default()),
        };

        (
            result.map(|_| Default::default()),
            gas_spent.saturating_add(decoding_cost),
        )
    }
}

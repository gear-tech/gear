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

//! # Gear BuiltIn Actors Pallet
//!
//! The BuiltIn Actors pallet provides a set of unique accounts that are treated as built-in
//! actors ids (`ProgramId`).
//!
//! - [`Config`]
//!
//! ## Overview
//!
//! The pallet implements a set of actors to handle messages from the message queue that
//! require some sort of runtime-related logic to shortcut the usual messages processing flow.
//! This provides an easy way for programs to interact with "pure runtime" actors thereby
//! allowing them to embed the blockchain abstractions provided by the Runtime.
//!
//! The list of available actors currently includes:
//! - `StakingProxy` actor to interact with the staking pallet from contracts.
//!
//! The pallet implements the [`pallet_gear::BuiltInActor`] trait so that it can be plugged into
//! the `Gear` pallet to intercept messages popped from the queue.
//!
//! The built-in actor does three things:
//! 1. Decodes the message payload into a respective known type;
//! 2. Based on the action encoded in the message, checks if the gas limit is respected;
//! 3. Prepares a call based on the message action/data and dispatches it.
//!
//! If an error occurs on either of the first two steps above, a Reply message would contain a
//! suitable error code. If the call is dispatched, the message is considered to have been
//! processed successfully regardless of the call outcome.
//!
//! The result of the built-in actor's processing is a set of `JournalNote`s - same like a
//! regular actor would produce. However, for the built-in actor only a limited subset of
//! `JournalNote`s is applicable (gas reservation, programs storage, programs rent, reply deposit,
//! awakening messages, pages/allocations updates are not applicable - TODO: validate this).
//!
//! Specifically, we'll have:
//! - `JournalNote::GasBurned` - the weight of extrinsic dispatched or 0 if no call was executed;
//! - `JournalNote::SendDispatch` - send auto-reply or error-reply message to the origin;
//! - `JournalNote::MessageDispatched` - signal end of processing;
//! - `JournalNote::MessageConsumed` - release reserved resources on the caller's side.
//!
//! Note: we don't create any `JournalNote::SendValue` entries because the value is handled
//! directly at the pallet level.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::items_after_test_module)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use alloc::string::ToString;
use common::Origin;
use core_processor::common::{DispatchOutcome, JournalNote};
use enum_iterator::Sequence;
use frame_support::{
    dispatch::{
        extract_actual_weight, DispatchInfo, Dispatchable, GetDispatchInfo, PostDispatchInfo,
    },
    traits::Get,
    PalletId,
};
use gear_builtin_actor_common::staking::*;
use gear_core::{
    ids::{MessageId, ProgramId},
    message::{Dispatch, ReplyMessage, ReplyPacket, StoredDispatch},
};
use gear_core_errors::SimpleExecutionError;
use pallet_gear::BuiltInActor;
use pallet_staking::RewardDestination;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_io::hashing::blake2_256;
use sp_runtime::traits::{StaticLookup, TrailingZeroInput, UniqueSaturatedInto};
use sp_std::{collections::btree_set::BTreeSet, prelude::*};
pub use weights::WeightInfo;

pub use pallet::*;

pub mod bls12_381;

pub type BalanceOf<T> = <T as pallet_staking::Config>::CurrencyBalance;

#[allow(dead_code)]
const LOG_TARGET: &str = "gear::builtin-actor";

/// Available actor types.
#[derive(Decode, Encode, Debug, Clone, Copy, PartialEq, Eq, Sequence, TypeInfo)]
#[repr(u32)]
pub enum ActorType {
    StakingProxy = 100,
    Bls12_381 = 101,
}

/// Built-in actor error
#[derive(Encode, Decode, Debug, PartialEq, Eq, PartialOrd, Ord, derive_more::Display)]
pub enum BuiltInActorReason {
    /// Not enough gas to dispatch a call.
    #[display(fmt = "Not enough gas to dispatch a call")]
    InsufficientGas,
    /// Error transferring value due to insufficient funds or existence requirements violation.
    #[display(fmt = "Error transferring value")]
    TransferError,
    /// Protocol error (encoding/decoding)
    #[display(fmt = "Failure to decode message")]
    UnknownMessageType,
}

impl From<BuiltInActorReason> for SimpleExecutionError {
    /// Convert [`BuiltInActorReason`] into [`gear_core_errors::SimpleExecutionError`].
    // TODO: think of a better mapping.
    fn from(reason: BuiltInActorReason) -> Self {
        match reason {
            BuiltInActorReason::InsufficientGas => SimpleExecutionError::RanOutOfGas,
            BuiltInActorReason::TransferError => SimpleExecutionError::UserspacePanic,
            BuiltInActorReason::UnknownMessageType => SimpleExecutionError::UserspacePanic,
        }
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    /// The current storage version.
    const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_staking::Config {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The overarching call type.
        type RuntimeCall: Parameter
            + Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
            + GetDispatchInfo
            + From<pallet_staking::Call<Self>>;

        /// Weight information to calculate the weight of the `handle()` function call.
        type WeightInfo: WeightInfo;

        /// The built-in actor pallet id, used for deriving its sovereign account ID.
        #[pallet::constant]
        type PalletId: Get<PalletId>;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Message executed.
        MessageExecuted { result: DispatchResult },
    }

    /// Cached built-in actor program ids to spare redundant computation.
    #[pallet::storage]
    #[pallet::getter(fn actors)]
    pub type Actors<T> = StorageMap<_, Identity, ActorType, ProgramId>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    impl<T: Config> Pallet<T> {
        /// The ID of the Staking Proxy built-in actor.
        ///
        /// This does computations, and is used in each block therefore we
        /// cache the value to ensure this function is only called once.
        pub fn staking_proxy_actor_id() -> ProgramId {
            if let Some(actor_id) = Self::actors(ActorType::StakingProxy) {
                return actor_id;
            }
            let entropy =
                (T::PalletId::get(), ActorType::StakingProxy as u32).using_encoded(blake2_256);
            let actor_id = Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
                .expect("infinite length input; no invalid inputs for type; qed");
            Actors::<T>::insert(ActorType::StakingProxy, actor_id);
            actor_id
        }

        /// The ID of the Bls12_381 built-in actor.
        ///
        /// This does computations, and is used in each block therefore we
        /// cache the value to ensure this function is only called once.
        pub fn bls12_381_actor_id() -> ProgramId {
            if let Some(actor_id) = Self::actors(ActorType::Bls12_381) {
                return actor_id;
            }

            let entropy =
                (T::PalletId::get(), ActorType::Bls12_381 as u32).using_encoded(blake2_256);
            let actor_id = Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
                .expect("infinite length input; no invalid inputs for type; qed");
            Actors::<T>::insert(ActorType::Bls12_381, actor_id);

            actor_id
        }
    }
}

impl<T: Config> BuiltInActor<ProgramId> for Pallet<T>
where
    T::AccountId: Origin,
{
    type Message = StoredDispatch;
    type Output = JournalNote;

    fn ids() -> BTreeSet<ProgramId> {
        enum_iterator::all::<ActorType>()
            .map(|t| match t {
                ActorType::StakingProxy => Self::staking_proxy_actor_id(),
                ActorType::Bls12_381 => Self::bls12_381_actor_id(),
            })
            .collect()
    }

    /// Handle a message from the queue as a normal actor would.
    fn handle(dispatch: StoredDispatch, gas_limit: u64) -> Vec<JournalNote> {
        let mut output = vec![];
        let actor_id = dispatch.destination();
        if actor_id == Self::staking_proxy_actor_id() {
            output = staking_proxy::handle::<T>(&dispatch, gas_limit).map_or_else(
                |e| {
                    Self::process_error(
                        &dispatch,
                        <T as Config>::WeightInfo::base_handle_weight().ref_time(),
                        e,
                    )
                },
                |(gas_spent, dispatch_result)| {
                    let gas_burned = <T as Config>::WeightInfo::base_handle_weight()
                        .ref_time()
                        .saturating_add(gas_spent);

                    let message_id = dispatch.id();
                    let origin = dispatch.source();
                    let actor_id = dispatch.destination();

                    // Build the reply message
                    let response: StakingResponse =
                        dispatch_result.err().map(Err).unwrap_or(Ok(())).into();
                    let payload = response
                        .encode()
                        .try_into()
                        .unwrap_or_else(|_| unreachable!("Response message is too large"));
                    let reply_id = MessageId::generate_reply(message_id);
                    let packet = ReplyPacket::new(payload, 0);
                    let reply_dispatch = ReplyMessage::from_packet(reply_id, packet)
                        .into_dispatch(actor_id, origin, message_id);

                    Self::process_success(message_id, origin, gas_burned, reply_dispatch)
                },
            );
        } else if actor_id == Self::bls12_381_actor_id() {
            let (gas_spent, result) = bls12_381::handle::<T>(&dispatch, gas_limit);
            output = match result {
                Ok(response) => {
                    let message_id = dispatch.id();
                    let origin = dispatch.source();
                    let actor_id = dispatch.destination();

                    // Build the reply message
                    let payload = response
                        .encode()
                        .try_into()
                        .unwrap_or_else(|_| unreachable!("Response message is too large"));
                    let reply_id = MessageId::generate_reply(message_id);
                    let packet = ReplyPacket::new(payload, 0);
                    let reply_dispatch = ReplyMessage::from_packet(reply_id, packet)
                        .into_dispatch(actor_id, origin, message_id);

                    Self::process_success(message_id, origin, gas_spent, reply_dispatch)
                }
                Err(e) => Self::process_error(&dispatch, gas_spent, e),
            };
        } else {
            log::debug!(
                target: LOG_TARGET,
                "Unknown built-in actor id: {:?}",
                actor_id,
            );
        }

        output
    }
}

impl<T: Config> Pallet<T>
where
    T::AccountId: Origin,
{
    // Successful call, generates the reply message to be sent out
    fn process_success(
        message_id: MessageId,
        origin: ProgramId,
        gas_burned: u64,
        reply_dispatch: Dispatch,
    ) -> Vec<JournalNote> {
        vec![
            JournalNote::GasBurned {
                message_id,
                amount: gas_burned,
            },
            JournalNote::SendDispatch {
                message_id,
                dispatch: reply_dispatch,
                delay: 0,
                reservation: None,
            },
            JournalNote::MessageDispatched {
                message_id,
                source: origin,
                outcome: DispatchOutcome::Success,
            },
            JournalNote::MessageConsumed(message_id),
        ]
    }

    // Error in the actor, generates error reply
    fn process_error(
        dispatch: &StoredDispatch,
        gas_burned: u64,
        err: BuiltInActorReason,
    ) -> Vec<JournalNote> {
        let message_id = dispatch.id();
        let origin = dispatch.source();
        let actor_id = dispatch.destination();
        let err_payload = err
            .to_string()
            .into_bytes()
            .try_into()
            .unwrap_or_else(|_| unreachable!("Error message is too large"));
        let err: SimpleExecutionError = err.into();

        vec![
            JournalNote::GasBurned {
                message_id,
                amount: gas_burned,
            },
            JournalNote::SendDispatch {
                message_id,
                dispatch: ReplyMessage::system(message_id, err_payload, err)
                    .into_dispatch(actor_id, origin, message_id),
                delay: 0,
                reservation: None,
            },
            JournalNote::MessageDispatched {
                message_id,
                source: origin,
                outcome: DispatchOutcome::MessageTrap {
                    program_id: actor_id,
                    trap: err.to_string(),
                },
            },
            JournalNote::MessageConsumed(message_id),
        ]
    }

    fn check_gas_limit(gas_limit: u64, info: &DispatchInfo) -> Result<(), BuiltInActorReason> {
        let weight = info.weight;
        if gas_limit < weight.ref_time() {
            return Err(BuiltInActorReason::InsufficientGas);
        }
        Ok(())
    }

    #[cfg(not(feature = "runtime-benchmarks"))]
    fn dispatch_call(
        origin: T::AccountId,
        call: <T as Config>::RuntimeCall,
        gas_limit: u64,
    ) -> Result<(u64, Result<(), StakingErrorReason>), BuiltInActorReason> {
        let call_info = call.get_dispatch_info();
        Self::check_gas_limit(gas_limit, &call_info)?;
        // Execute call
        let res = call.dispatch(frame_system::RawOrigin::Signed(origin).into());
        let actual_gas = extract_actual_weight(&res, &call_info).ref_time();
        Self::deposit_event(Event::MessageExecuted {
            result: res.map(|_| ()).map_err(|e| e.error),
        });
        match res {
            Ok(_post_info) => {
                log::debug!(
                    target: LOG_TARGET,
                    "Call dispatched successfully",
                );
                Ok((actual_gas, Ok(())))
            }
            Err(e) => {
                log::error!(
                    target: LOG_TARGET,
                    "Error disptaching call: {:?}",
                    e,
                );
                Ok((actual_gas, Err(StakingErrorReason::DispatchError)))
            }
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn dispatch_call(
        _origin: T::AccountId,
        call: <T as Config>::RuntimeCall,
        gas_limit: u64,
    ) -> Result<(u64, Result<(), StakingErrorReason>), BuiltInActorReason> {
        let call_info = call.get_dispatch_info();
        Self::check_gas_limit(gas_limit, &call_info)?;
        // Skipping the actual call dispatch
        let _ = extract_actual_weight(&Ok(Default::default()), &call_info).ref_time();
        Self::deposit_event(Event::MessageExecuted { result: Ok(()) });

        Ok((0, Ok(())))
    }
}

pub mod staking_proxy {
    use super::*;

    pub fn handle<T: Config>(
        dispatch: &StoredDispatch,
        gas_limit: u64,
    ) -> Result<(u64, Result<(), StakingErrorReason>), BuiltInActorReason>
    where
        T::AccountId: Origin,
    {
        let message = dispatch.message();
        let origin = <T::AccountId as Origin>::from_origin(dispatch.source().into_origin());

        // Decode the message payload to derive the desired action
        let msg: StakingMessage = Decode::decode(&mut message.payload_bytes())
            .map_err(|_| BuiltInActorReason::UnknownMessageType)?;
        let call = match msg {
            StakingMessage::Bond { value } => pallet_staking::Call::<T>::bond {
                controller: T::Lookup::unlookup(origin.clone()),
                value: value.unique_saturated_into(),
                payee: RewardDestination::Stash,
            }
            .into(),
            StakingMessage::BondExtra { value } => pallet_staking::Call::<T>::bond_extra {
                max_additional: value.unique_saturated_into(),
            }
            .into(),
            StakingMessage::Unbond { value } => pallet_staking::Call::<T>::unbond {
                value: value.unique_saturated_into(),
            }
            .into(),
            StakingMessage::WithdrawUnbonded { num_slashing_spans } => {
                pallet_staking::Call::<T>::withdraw_unbonded { num_slashing_spans }.into()
            }
            StakingMessage::Nominate { targets } => pallet_staking::Call::<T>::nominate {
                targets: targets
                    .into_iter()
                    .map(|account_id| {
                        let origin = <T::AccountId as Origin>::from_origin(
                            ProgramId::from(&account_id[..]).into_origin(),
                        );
                        T::Lookup::unlookup(origin)
                    })
                    .collect(),
            }
            .into(),
            StakingMessage::PayoutStakers {
                validator_stash,
                era,
            } => {
                let stash_id = <T::AccountId as Origin>::from_origin(
                    ProgramId::from(&validator_stash[..]).into_origin(),
                );
                pallet_staking::Call::<T>::payout_stakers {
                    validator_stash: stash_id,
                    era,
                }
                .into()
            }
            StakingMessage::Rebond { value } => pallet_staking::Call::<T>::rebond {
                value: value.unique_saturated_into(),
            }
            .into(),
        };
        Pallet::<T>::dispatch_call(origin, call, gas_limit)
    }
}

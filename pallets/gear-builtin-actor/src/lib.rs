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

//! # Gear Builtin Actors Pallet
//!
//! The Builtn Actors pallet provides a registry of the builtin actors available in the Runtime.
//!
//! - [`Config`]
//!
//! ## Overview
//!
//! The pallet implements the `pallet_gear::BuiltinRouter` allowing to restore builtin actors
//! claimed `BuiltinId`'s based on their corresponding `ProgramId` address.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::items_after_test_module)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub use weights::WeightInfo;

use alloc::{collections::BTreeMap, string::ToString};
use core_processor::{
    common::{DispatchOutcome, JournalNote},
    process_non_executable, SystemReservationContext,
};
use gear_core::{
    ids::{BuiltinId, MessageId, ProgramId},
    message::{DispatchKind, ReplyMessage, ReplyPacket, StoredDispatch},
};
use gear_core_errors::SimpleExecutionError;
use impl_trait_for_tuples::impl_for_tuples;
use pallet_gear::{BuiltinRouter, BuiltinRouterProvider};
use parity_scale_codec::{Decode, Encode};
use sp_io::hashing::blake2_256;
use sp_runtime::traits::{TrailingZeroInput, Zero};
use sp_std::prelude::*;

pub use pallet::*;

#[allow(dead_code)]
const LOG_TARGET: &str = "gear::builtin_actor";

pub trait Dispatchable {
    type Payload: AsRef<[u8]>;

    fn source(&self) -> ProgramId;
    fn destination(&self) -> BuiltinId;
    fn payload_bytes(&self) -> &[u8];
}

#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
pub struct SimpleBuiltinMessage {
    source: ProgramId,
    destination: BuiltinId,
    payload: Vec<u8>,
}

impl Dispatchable for SimpleBuiltinMessage {
    type Payload = Vec<u8>;

    fn source(&self) -> ProgramId {
        self.source
    }

    fn destination(&self) -> BuiltinId {
        self.destination
    }

    fn payload_bytes(&self) -> &[u8] {
        self.payload.as_ref()
    }
}

/// Built-in actor error type
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, derive_more::Display)]
pub enum BuiltinActorError {
    /// Occurs if the underlying call has the weight greater than the `gas_limit`.
    #[display(fmt = "Not enough gas supplied")]
    InsufficientGas,
    /// Occurs if the dispatch's message can't be decoded into a known type.
    #[display(fmt = "Failure to decode message")]
    UnknownMessageType,
    /// Occurs if a builtin id doesn't belong to any registered actor.
    #[display(fmt = "Unknown builtin id")]
    UnknownBuiltinId,
}

// TODO: adjust according to potential changes in `SimpleExecutionError` that might be coming
impl From<BuiltinActorError> for SimpleExecutionError {
    /// Convert [`BuiltinActorError`] into [`gear_core_errors::SimpleExecutionError`].
    fn from(err: BuiltinActorError) -> Self {
        match err {
            BuiltinActorError::InsufficientGas => SimpleExecutionError::RanOutOfGas,
            BuiltinActorError::UnknownMessageType => SimpleExecutionError::UserspacePanic,
            BuiltinActorError::UnknownBuiltinId => SimpleExecutionError::BackendError,
        }
    }
}

pub type BuiltinResult<P> = Result<P, BuiltinActorError>;

/// A trait representing an interface of a builtin actor that can receive a message
/// and produce a set of outputs that can then be converted into a reply message.
pub trait BuiltinActor<Message: Dispatchable, Gas: Zero> {
    /// Handles a message and returns a result and the actual gas spent.
    fn handle(message: &Message, gas_limit: Gas) -> (BuiltinResult<Message::Payload>, Gas);

    fn get_ids(buffer: &mut Vec<BuiltinId>);
}

pub trait RegisteredBuiltinActor<M: Dispatchable, G: Zero>: BuiltinActor<M, G> {
    /// The global unique ID of the trait implementer type.
    const ID: BuiltinId;
}

// Assuming as many as 16 builtin actors for the meantime
#[impl_for_tuples(16)]
#[tuple_types_custom_trait_bound(RegisteredBuiltinActor<M, G>)]
impl<M: Dispatchable, G: Zero> BuiltinActor<M, G> for Tuple {
    fn handle(message: &M, gas_limit: G) -> (BuiltinResult<M::Payload>, G) {
        for_tuples!(
            #(
                if (Tuple::ID == message.destination()) {
                    return Tuple::handle(message, gas_limit);
                }
            )*
        );
        (Err(BuiltinActorError::UnknownBuiltinId), Zero::zero())
    }

    fn get_ids(buffer: &mut Vec<BuiltinId>) {
        for_tuples!(
            #(
                buffer.push(Tuple::ID);
            )*
        );
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, traits::Get, PalletId};
    use frame_system::pallet_prelude::*;

    // This pallet doesn't define a storage version because it doesn't use any storage
    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The builtin actor type.
        type BuiltinActor: BuiltinActor<SimpleBuiltinMessage, u64>;

        /// Weight cost incurred by builtin actors calls.
        type WeightInfo: WeightInfo;

        /// The builtin actor pallet id, used for deriving unique actors ids.
        #[pallet::constant]
        type PalletId: Get<PalletId>;
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    impl<T: Config> Pallet<T> {
        /// Generate an `actor_id` given a builtin ID.
        ///
        ///
        /// This does computations, therefore we should seek to cache the value at the time of
        /// a builtin actor registration.
        pub fn generate_actor_id(builtin_id: BuiltinId) -> ProgramId {
            let entropy = (T::PalletId::get(), builtin_id).using_encoded(blake2_256);
            let actor_id = Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
                .expect("infinite length input; no invalid inputs for type; qed");
            actor_id
        }

        pub fn process_success(
            dispatch: &StoredDispatch,
            gas_spent: u64,
            response_bytes: Vec<u8>,
        ) -> Vec<JournalNote> {
            let message_id = dispatch.id();
            let origin = dispatch.source();
            let actor_id = dispatch.destination();

            let mut journal = vec![];

            journal.push(JournalNote::GasBurned {
                message_id,
                amount: gas_spent,
            });

            // Build the reply message
            let payload = response_bytes
                .try_into()
                .unwrap_or_else(|_| unreachable!("Response message is too large"));
            let reply_id = MessageId::generate_reply(message_id);
            let packet = ReplyPacket::new(payload, 0);
            let dispatch = ReplyMessage::from_packet(reply_id, packet)
                .into_dispatch(actor_id, origin, message_id);

            journal.push(JournalNote::SendDispatch {
                message_id,
                dispatch,
                delay: 0,
                reservation: None,
            });

            let outcome = DispatchOutcome::Success;
            journal.push(JournalNote::MessageDispatched {
                message_id,
                source: origin,
                outcome,
            });

            journal.push(JournalNote::MessageConsumed(message_id));

            journal
        }

        // Error in the actor, generates error reply
        pub fn process_error(
            dispatch: &StoredDispatch,
            gas_spent: u64,
            err: BuiltinActorError,
        ) -> Vec<JournalNote> {
            let message_id = dispatch.id();
            let origin = dispatch.source();
            let actor_id = dispatch.destination();

            let mut journal = vec![];

            journal.push(JournalNote::GasBurned {
                message_id,
                amount: gas_spent,
            });

            let err_payload = err
                .to_string()
                .into_bytes()
                .try_into()
                .unwrap_or_else(|_| unreachable!("Error message is too large"));
            let err: SimpleExecutionError = err.into();

            let dispatch = ReplyMessage::system(message_id, err_payload, err)
                .into_dispatch(actor_id, origin, message_id);

            journal.push(JournalNote::SendDispatch {
                message_id,
                dispatch,
                delay: 0,
                reservation: None,
            });

            let outcome = DispatchOutcome::MessageTrap {
                program_id: actor_id,
                trap: err.to_string(),
            };
            journal.push(JournalNote::MessageDispatched {
                message_id,
                source: origin,
                outcome,
            });

            journal.push(JournalNote::MessageConsumed(message_id));

            journal
        }
    }
}

impl<T: Config> BuiltinRouterProvider<StoredDispatch, JournalNote, u64> for Pallet<T> {
    type Router = BuiltinRegistry<T>;

    fn provide() -> BuiltinRegistry<T> {
        BuiltinRegistry::<T>::new()
    }

    fn provision_cost() -> u64 {
        <T as Config>::WeightInfo::provide().ref_time()
    }
}

pub struct BuiltinRegistry<T: Config> {
    pub registry: BTreeMap<ProgramId, BuiltinId>,
    pub _phantom: sp_std::marker::PhantomData<T>,
}
impl<T: Config> BuiltinRegistry<T> {
    fn new() -> Self {
        let mut registry = BTreeMap::new();
        let mut builtin_ids = Vec::with_capacity(16);
        <T as Config>::BuiltinActor::get_ids(&mut builtin_ids);
        let builtin_ids_len = builtin_ids.len();
        for builtin_id in builtin_ids {
            let actor_id = Pallet::<T>::generate_actor_id(builtin_id);
            registry.entry(actor_id).or_insert(builtin_id);
        }
        assert!(
            registry.len() == builtin_ids_len,
            "Duplicate builtin ids detected!"
        );

        Self {
            registry,
            _phantom: Default::default(),
        }
    }
}

impl<T: Config> BuiltinRouter for BuiltinRegistry<T> {
    type QueuedDispatch = StoredDispatch;
    type Output = JournalNote;

    fn lookup(&self, id: &ProgramId) -> Option<BuiltinId> {
        self.registry.get(id).copied()
    }

    fn dispatch(&self, dispatch: StoredDispatch, gas_limit: u64) -> Option<Vec<JournalNote>> {
        let actor_id = dispatch.destination();
        let Some(builtin_id) = self.registry.get(&actor_id) else {
            return None;
        };

        // Builtin actors can only execute `handle` dispatches; all other cases yield
        // `no-execution` outcome.
        if dispatch.kind() != DispatchKind::Handle {
            let dispatch = dispatch.into_incoming(gas_limit);
            let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);
            return Some(process_non_executable(
                dispatch,
                actor_id,
                system_reservation_ctx,
            ));
        }
        // Re-package the dispatch into a `SimpleBuiltinMessage` for the builtin actor
        let builtin_message = SimpleBuiltinMessage {
            source: dispatch.source(),
            destination: *builtin_id,
            payload: dispatch.payload_bytes().to_vec(),
        };

        // Do message processing
        let (res, gas_spent) = <T as Config>::BuiltinActor::handle(&builtin_message, gas_limit);
        // We rely on a builtin actor having performed the check for gas limit consistency
        // and having reported an error if the `gas_limit` was to have been exceeded.
        // However, to avoid gas tree corruption error, we must not report as spent more gas than
        // the amount reserved in gas tree (that is, `gas_limit`). Hence (just in case):
        let gas_spent = gas_spent.min(gas_limit);
        Some(
            res.map_or_else(
                |err| {
                    log::debug!(target: LOG_TARGET, "Builtin actor error: {:?}", err);
                    Pallet::<T>::process_error(&dispatch, gas_spent, err)
                },
                |response_bytes| {
                    log::debug!(target: LOG_TARGET, "Builtin call dispatched successfully");
                    Pallet::<T>::process_success(&dispatch, gas_spent, response_bytes)
                },
            )
            .into_iter()
            .collect(),
        )
    }
}

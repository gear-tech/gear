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
//! The pallet implements the `pallet_gear::BuiltinDispatcher` allowing to restore builtin actors
//! claimed `BuiltinId`'s based on their corresponding `ProgramId` address.

#![cfg_attr(not(feature = "std"), no_std)]

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
    common::{ActorExecutionErrorReplyReason, DispatchResult, JournalNote, TrapExplanation},
    process_execution_error, process_success, SuccessfulDispatchResultKind,
    SystemReservationContext,
};
use gear_core::{
    gas::GasCounter,
    ids::{hash, BuiltinId, ProgramId},
    message::{
        ContextOutcomeDrain, DispatchKind, MessageContext, Payload, ReplyPacket, StoredDispatch,
    },
};
use impl_trait_for_tuples::impl_for_tuples;
use pallet_gear::{BuiltinDispatcher, BuiltinDispatcherProvider};
use parity_scale_codec::{Decode, Encode};
use sp_runtime::traits::Zero;
use sp_std::prelude::*;

pub use pallet::*;

const LOG_TARGET: &str = "gear::builtin";

/// Built-in actor error type
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, derive_more::Display)]
pub enum BuiltinActorError {
    /// Occurs if the underlying call has the weight greater than the `gas_limit`.
    #[display(fmt = "Not enough gas supplied")]
    InsufficientGas,
    /// Occurs if the dispatch's message can't be decoded into a known type.
    #[display(fmt = "Failure to decode message")]
    UnknownMessageType,
    /// Indicates an unreachable (under normal conditions) state:
    /// - the `BuiltinId` passed to the `handle()` function does not match any known actors.
    #[display(fmt = "Unknown builtin id passed to the `handle()` method")]
    UnknownActor,
}

impl From<BuiltinActorError> for ActorExecutionErrorReplyReason {
    /// Convert [`BuiltinActorError`] to [`core_processor::common::ActorExecutionErrorReplyReason`]
    fn from(err: BuiltinActorError) -> Self {
        match err {
            BuiltinActorError::InsufficientGas => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::GasLimitExceeded)
            }
            BuiltinActorError::UnknownMessageType => ActorExecutionErrorReplyReason::Trap(
                TrapExplanation::Panic("Message decoding error".to_string().into()),
            ),
            // This should convey a message of hitting an unreachable state
            BuiltinActorError::UnknownActor => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::Unknown)
            }
        }
    }
}

pub type BuiltinResult<P> = Result<P, BuiltinActorError>;

/// A trait representing an interface of a builtin actor that can receive a message
/// and produce a set of outputs that can then be converted into a reply message.
pub trait BuiltinActor<Gas: Zero> {
    /// Handles a message and returns a result and the actual gas spent.
    fn handle(
        builtin_id: BuiltinId,
        message: &StoredDispatch,
        gas_limit: Gas,
    ) -> (BuiltinResult<Payload>, Gas);

    fn get_ids(buffer: &mut Vec<BuiltinId>);
}

pub trait RegisteredBuiltinActor<G: Zero>: BuiltinActor<G> {
    /// The global unique ID of the trait implementer type.
    const ID: BuiltinId;
}

// Assuming as many as 16 builtin actors for the meantime
#[impl_for_tuples(16)]
#[tuple_types_custom_trait_bound(RegisteredBuiltinActor<G>)]
impl<G: Zero> BuiltinActor<G> for Tuple {
    fn handle(
        builtin_id: BuiltinId,
        message: &StoredDispatch,
        gas_limit: G,
    ) -> (BuiltinResult<Payload>, G) {
        for_tuples!(
            #(
                if (Tuple::ID == builtin_id) {
                    return Tuple::handle(builtin_id, message, gas_limit);
                }
            )*
        );
        (Err(BuiltinActorError::UnknownActor), Zero::zero())
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
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    pub(crate) const SEED: [u8; 8] = *b"built/in";

    // This pallet doesn't define a storage version because it doesn't use any storage
    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The builtin actor type.
        type BuiltinActor: BuiltinActor<u64>;

        /// Weight cost incurred by builtin actors calls.
        type WeightInfo: WeightInfo;
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
            hash((SEED, builtin_id).encode().as_slice()).into()
        }
    }
}

impl<T: Config> BuiltinDispatcherProvider<StoredDispatch, u64> for Pallet<T> {
    type Dispatcher = BuiltinRegistry<T>;

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

impl<T: Config> BuiltinDispatcher for BuiltinRegistry<T> {
    type QueuedDispatch = StoredDispatch;

    fn lookup(&self, id: &ProgramId) -> Option<BuiltinId> {
        self.registry.get(id).copied()
    }

    fn dispatch(
        &self,
        builtin_id: BuiltinId,
        dispatch: StoredDispatch,
        gas_limit: u64,
    ) -> Vec<JournalNote> {
        let actor_id = dispatch.destination();

        // Builtin actors can only execute dispatches of `Handle` kind and only `Handle`
        // dispatches can end up here (TODO: elaborate).
        if dispatch.kind() != DispatchKind::Handle {
            unreachable!("Only handle dispatches can end up here");
        }

        let mut gas_counter = GasCounter::new(gas_limit);

        let (res, gas_spent) =
            <T as Config>::BuiltinActor::handle(builtin_id, &dispatch, gas_limit);

        // We rely on a builtin actor having performed the check for gas limit consistency
        // and having reported an error if the `gas_limit` was to have been exceeded.
        // However, to avoid gas tree corruption error, we must not report as spent more gas than
        // the amount reserved in gas tree (that is, `gas_limit`). Hence (just in case):
        let gas_spent = gas_spent.min(gas_limit);

        // This should always return `ChargeResult::Enough` now thanks to the above check.
        let _ = gas_counter.charge(gas_spent);

        let dispatch = dispatch.into_incoming(gas_limit);

        match res {
            Ok(response_payload) => {
                log::debug!(target: LOG_TARGET, "Builtin call dispatched successfully");

                let mut dispatch_result =
                    DispatchResult::success(dispatch.clone(), actor_id, gas_counter.to_amount());

                // Create an artificial `MessageContext` object that will help us to generate
                // a reply from the builtin actor.
                let mut message_context =
                    MessageContext::new(dispatch, actor_id, Default::default());
                let packet = ReplyPacket::new(response_payload, 0);

                // Mark reply as sent
                if let Ok(_reply_id) = message_context.reply_commit(packet.clone(), None) {
                    let (outcome, context_store) = message_context.drain();

                    dispatch_result.context_store = context_store;
                    let ContextOutcomeDrain {
                        outgoing_dispatches: generated_dispatches,
                        ..
                    } = outcome.drain();
                    dispatch_result.generated_dispatches = generated_dispatches;
                };

                process_success(SuccessfulDispatchResultKind::Success, dispatch_result)
            }
            Err(err) => {
                log::debug!(target: LOG_TARGET, "Builtin actor error: {:?}", err);
                let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);
                process_execution_error(dispatch, actor_id, gas_spent, system_reservation_ctx, err)
            }
        }
    }
}

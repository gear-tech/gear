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
#![allow(clippy::manual_inspect)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

pub mod bls12_381;
pub mod staking;
pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub use weights::WeightInfo;

use alloc::{
    collections::{btree_map::Entry, BTreeMap},
    string::ToString,
};
use core::marker::PhantomData;
use core_processor::{
    common::{ActorExecutionErrorReplyReason, DispatchResult, JournalNote, TrapExplanation},
    process_execution_error, process_success, SuccessfulDispatchResultKind,
    SystemReservationContext,
};
use gear_core::{
    gas::GasCounter,
    ids::{hash, ProgramId},
    message::{
        ContextOutcomeDrain, DispatchKind, MessageContext, Payload, ReplyPacket, StoredDispatch,
    },
    str::LimitedStr,
};
use impl_trait_for_tuples::impl_for_tuples;
use pallet_gear::{BuiltinDispatcher, BuiltinDispatcherFactory, HandleFn};
use parity_scale_codec::{Decode, Encode};
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
    DecodingError,
    /// Actor's inner error encoded as a String.
    #[display(fmt = "Builtin execution resulted in error: {_0}")]
    Custom(LimitedStr<'static>),
}

impl From<BuiltinActorError> for ActorExecutionErrorReplyReason {
    /// Convert [`BuiltinActorError`] to [`core_processor::common::ActorExecutionErrorReplyReason`]
    fn from(err: BuiltinActorError) -> Self {
        match err {
            BuiltinActorError::InsufficientGas => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::GasLimitExceeded)
            }
            BuiltinActorError::DecodingError => ActorExecutionErrorReplyReason::Trap(
                TrapExplanation::Panic("Message decoding error".to_string().into()),
            ),
            BuiltinActorError::Custom(e) => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::Panic(e))
            }
        }
    }
}

/// A trait representing an interface of a builtin actor that can handle a message
/// from message queue (a `StoredDispatch`) to produce an outcome and gas spent.
pub trait BuiltinActor {
    type Error;

    /// Handles a message and returns a result and the actual gas spent.
    fn handle(dispatch: &StoredDispatch, gas_limit: u64) -> (Result<Payload, Self::Error>, u64);
}

/// A marker struct to associate a builtin actor with its unique ID.
pub struct ActorWithId<const ID: u64, A: BuiltinActor>(PhantomData<A>);

/// Glue trait to implement `BuiltinCollection` for a tuple of `ActorWithId`.
trait BuiltinActorWithId {
    const ID: u64;

    type Error;
    type Actor: BuiltinActor<Error = Self::Error>;
}

impl<const ID: u64, A: BuiltinActor> BuiltinActorWithId for ActorWithId<ID, A> {
    const ID: u64 = ID;

    type Error = A::Error;
    type Actor = A;
}

/// A trait defining a method to convert a tuple of `BuiltinActor` types into
/// a in-memory collection of builtin actors.
pub trait BuiltinCollection<E> {
    fn collect(
        registry: &mut BTreeMap<ProgramId, Box<HandleFn<E>>>,
        id_converter: &dyn Fn(u64) -> ProgramId,
    );
}

// Assuming as many as 16 builtin actors for the meantime
#[impl_for_tuples(16)]
#[tuple_types_custom_trait_bound(BuiltinActorWithId<Error = E> + 'static)]
impl<E> BuiltinCollection<E> for Tuple {
    fn collect(
        registry: &mut BTreeMap<ProgramId, Box<HandleFn<E>>>,
        id_converter: &dyn Fn(u64) -> ProgramId,
    ) {
        for_tuples!(
            #(
                let actor_id = id_converter(Tuple::ID);
                if let Entry::Vacant(e) = registry.entry(actor_id) {
                    e.insert(Box::new(Tuple::Actor::handle));
                } else {
                    unreachable!("Duplicate builtin ids");
                }
            )*
        );
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{
        dispatch::{GetDispatchInfo, PostDispatchInfo},
        pallet_prelude::*,
    };
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::Dispatchable;

    pub(crate) const SEED: [u8; 8] = *b"built/in";

    // This pallet doesn't define a storage version because it doesn't use any storage
    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The overarching call type.
        type RuntimeCall: Parameter
            + Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
            + GetDispatchInfo;

        /// The builtin actor type.
        type Builtins: BuiltinCollection<BuiltinActorError>;

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
        pub fn generate_actor_id(builtin_id: u64) -> ProgramId {
            hash((SEED, builtin_id).encode().as_slice()).into()
        }
    }
}

impl<T: Config> BuiltinDispatcherFactory for Pallet<T> {
    type Error = BuiltinActorError;
    type Output = BuiltinRegistry<T>;

    fn create() -> (BuiltinRegistry<T>, u64) {
        (
            BuiltinRegistry::<T>::new(),
            <T as Config>::WeightInfo::create_dispatcher().ref_time(),
        )
    }
}

pub struct BuiltinRegistry<T: Config> {
    pub registry: BTreeMap<ProgramId, Box<HandleFn<BuiltinActorError>>>,
    pub _phantom: sp_std::marker::PhantomData<T>,
}
impl<T: Config> BuiltinRegistry<T> {
    fn new() -> Self {
        let mut registry = BTreeMap::new();
        <T as Config>::Builtins::collect(&mut registry, &Pallet::<T>::generate_actor_id);

        Self {
            registry,
            _phantom: Default::default(),
        }
    }
}

impl<T: Config> BuiltinDispatcher for BuiltinRegistry<T> {
    type Error = BuiltinActorError;

    fn lookup<'a>(&'a self, id: &ProgramId) -> Option<&'a HandleFn<Self::Error>> {
        self.registry.get(id).map(|f| &**f)
    }

    fn run(
        &self,
        f: &HandleFn<Self::Error>,
        dispatch: StoredDispatch,
        gas_limit: u64,
    ) -> Vec<JournalNote> {
        let actor_id = dispatch.destination();

        if dispatch.kind() != DispatchKind::Handle {
            unreachable!("Only handle dispatches can end up here");
        }
        if dispatch.context().is_some() {
            unreachable!("Builtin actors can't have context from earlier executions");
        }

        // Creating a gas counter to track gas usage (because core processor needs it).
        let mut gas_counter = GasCounter::new(gas_limit);

        // TODO: #3752. Need to run gas limit check before calling `f(&dispatch)`.

        // Actual call to the builtin actor
        let (res, gas_spent) = f(&dispatch, gas_limit);

        // We rely on a builtin actor to perform the check for gas limit consistency before
        // executing a message and report an error if the `gas_limit` was to have been exceeded.
        // However, to avoid gas tree corruption error, we must not report as spent more gas than
        // the amount reserved in gas tree (that is, `gas_limit`). Hence (just in case):
        let gas_spent = gas_spent.min(gas_limit);

        // Let the `gas_counter` know how much gas was spent.
        let _ = gas_counter.charge(gas_spent);

        let dispatch = dispatch.into_incoming(gas_limit);

        match res {
            Ok(response_payload) => {
                // Builtin actor call was successful and returned some payload.
                log::debug!(target: LOG_TARGET, "Builtin call dispatched successfully");

                let mut dispatch_result =
                    DispatchResult::success(dispatch.clone(), actor_id, gas_counter.to_amount());

                // Create an artificial `MessageContext` object that will help us to generate
                // a reply from the builtin actor.
                let mut message_context =
                    MessageContext::new(dispatch, actor_id, Default::default()).unwrap_or_else(
                        || {
                            unreachable!(
                                "Builtin actor can't have context stored,
                                 so must be always possible to create a new message context"
                            )
                        },
                    );
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
                    dispatch_result.reply_sent = true;
                } else {
                    unreachable!("Failed to send reply from builtin actor");
                };

                // Using the core processor logic create necessary `JournalNote`'s for us.
                process_success(SuccessfulDispatchResultKind::Success, dispatch_result)
            }
            Err(err) => {
                // Builtin actor call failed.
                log::debug!(target: LOG_TARGET, "Builtin actor error: {:?}", err);
                let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);
                // The core processor will take care of creating necessary `JournalNote`'s.
                process_execution_error(dispatch, actor_id, gas_spent, system_reservation_ctx, err)
            }
        }
    }
}

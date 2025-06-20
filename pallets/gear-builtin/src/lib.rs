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

//! # Gear Builtin Actors Pallet
//!
//! The Builtn Actors pallet provides a registry of the builtin actors available in the Runtime.
//!
//! - [`Config`]
//!
//! ## Overview
//!
//! The pallet implements the `pallet_gear::BuiltinDispatcher` allowing to restore builtin actors
//! claimed `BuiltinId`'s based on their corresponding `ActorId` address.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::manual_inspect)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

pub mod bls12_381;
pub mod migration;
pub mod proxy;
pub mod staking;
pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub use weights::WeightInfo;

use alloc::{
    collections::{btree_map::Entry, BTreeMap},
    format,
};
use common::{storage::Limiter, BlockLimiter};
use core::marker::PhantomData;
use core_processor::{
    common::{ActorExecutionErrorReplyReason, DispatchResult, JournalNote, TrapExplanation},
    process_allowance_exceed, process_execution_error, process_success,
    SuccessfulDispatchResultKind, SystemReservationContext,
};
use frame_support::{dispatch::extract_actual_weight, traits::StorageVersion};
use gear_core::{
    buffer::Payload,
    gas::{ChargeResult, GasAllowanceCounter, GasAmount, GasCounter},
    ids::ActorId,
    message::{
        ContextOutcomeDrain, DispatchKind, MessageContext, ReplyPacket, StoredDispatch, Value,
    },
    str::LimitedStr,
    utils::hash,
};
use impl_trait_for_tuples::impl_for_tuples;
pub use pallet::*;
use pallet_gear::{BuiltinDispatcher, BuiltinDispatcherFactory, BuiltinInfo, HandleFn, WeightFn};
use parity_scale_codec::{Decode, Encode};
use sp_std::prelude::*;

type CallOf<T> = <T as Config>::RuntimeCall;
pub type GasAllowanceOf<T> = <<T as Config>::BlockLimiter as BlockLimiter>::GasAllowance;

const LOG_TARGET: &str = "gear::builtin";

pub type ActorErrorHandleFn = HandleFn<BuiltinContext, BuiltinHandleResult, BuiltinActorError>;

/// Built-in actor error type
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, derive_more::Display)]
pub enum BuiltinActorError {
    /// Occurs if the underlying call has the weight greater than the `gas_limit`.
    #[display("Not enough gas supplied")]
    InsufficientGas,
    /// Occurs if the dispatch's message can't be decoded into a known type.
    #[display("Failure to decode message")]
    DecodingError,
    /// Actor's inner error encoded as a String.
    #[display("Builtin execution resulted in error: {_0}")]
    Custom(LimitedStr<'static>),
    /// Occurs if a builtin actor execution does not fit in the current block.
    #[display("Block gas allowance exceeded")]
    GasAllowanceExceeded,
}

impl From<BuiltinActorError> for ActorExecutionErrorReplyReason {
    /// Convert [`BuiltinActorError`] to [`core_processor::common::ActorExecutionErrorReplyReason`]
    fn from(err: BuiltinActorError) -> Self {
        match err {
            BuiltinActorError::InsufficientGas => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::GasLimitExceeded)
            }
            BuiltinActorError::DecodingError => ActorExecutionErrorReplyReason::Trap(
                TrapExplanation::Panic(LimitedStr::from_small_str("Message decoding error").into()),
            ),
            BuiltinActorError::Custom(e) => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::Panic(e.into()))
            }
            BuiltinActorError::GasAllowanceExceeded => {
                unreachable!("Never supposed to be converted to error reply reason")
            }
        }
    }
}

/// The result of a builtin actor handle call.
#[derive(Debug)]
pub struct BuiltinHandleResult {
    pub payload: Payload,
    pub used_value: Value,
}

/// A builtin actor execution context. Primarily used to track gas usage.
#[derive(Debug)]
pub struct BuiltinContext {
    pub(crate) gas_counter: GasCounter,
    pub(crate) gas_allowance_counter: GasAllowanceCounter,
}

impl BuiltinContext {
    // Tries to charge the gas amount from the gas counters.
    pub fn try_charge_gas(&mut self, amount: u64) -> Result<(), BuiltinActorError> {
        if self.gas_counter.charge_if_enough(amount) == ChargeResult::NotEnough {
            return Err(BuiltinActorError::InsufficientGas);
        }

        if self.gas_allowance_counter.charge_if_enough(amount) == ChargeResult::NotEnough {
            return Err(BuiltinActorError::GasAllowanceExceeded);
        }

        Ok(())
    }

    // Checks if an amount of gas can be charged without actually modifying the inner counters.
    pub fn can_charge_gas(&self, amount: u64) -> Result<(), BuiltinActorError> {
        if self.gas_counter.left() < amount {
            return Err(BuiltinActorError::InsufficientGas);
        }

        if self.gas_allowance_counter.left() < amount {
            return Err(BuiltinActorError::GasAllowanceExceeded);
        }

        Ok(())
    }

    fn to_gas_amount(&self) -> GasAmount {
        self.gas_counter.to_amount()
    }
}

/// A trait representing an interface of a builtin actor that can handle a message
/// from message queue (a `StoredDispatch`) to produce an outcome and gas spent.
pub trait BuiltinActor {
    /// Handles a message and returns a result and the actual gas spent.
    fn handle(
        dispatch: &StoredDispatch,
        context: &mut BuiltinContext,
    ) -> Result<BuiltinHandleResult, BuiltinActorError>;

    /// Returns the maximum gas that can be spent by the actor.
    fn max_gas() -> u64;
}

/// A marker struct to associate a builtin actor with its unique ID.
pub struct ActorWithId<const ID: u64, A: BuiltinActor>(PhantomData<A>);

/// Glue trait to implement `BuiltinCollection` for a tuple of `ActorWithId`.
trait BuiltinActorWithId {
    const ID: u64;
    type Actor: BuiltinActor;
}

impl<const ID: u64, A: BuiltinActor> BuiltinActorWithId for ActorWithId<ID, A> {
    const ID: u64 = ID;
    type Actor = A;
}

/// A trait defining a method to convert a tuple of `BuiltinActor` types into
/// a in-memory collection of builtin actors.
pub trait BuiltinCollection {
    fn collect(
        registry: &mut BTreeMap<ActorId, (Box<ActorErrorHandleFn>, Box<WeightFn>)>,
        id_converter: &dyn Fn(u64) -> ActorId,
    );
}

// Assuming as many as 16 builtin actors for the meantime
#[impl_for_tuples(16)]
#[tuple_types_custom_trait_bound(BuiltinActorWithId + 'static)]
impl BuiltinCollection for Tuple {
    fn collect(
        registry: &mut BTreeMap<ActorId, (Box<ActorErrorHandleFn>, Box<WeightFn>)>,
        id_converter: &dyn Fn(u64) -> ActorId,
    ) {
        for_tuples!(
            #(
                let actor_id = id_converter(Tuple::ID);
                if let Entry::Vacant(e) = registry.entry(actor_id) {
                    e.insert((Box::new(Tuple::Actor::handle), Box::new(Tuple::Actor::max_gas)));
                } else {
                    let err_msg = format!(
                        "Tuple::for_tuples: Duplicate builtin ids. \
                        Actor id - {actor_id}"
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}")
                }
            )*
        );
    }
}

/// The current storage version.
pub(crate) const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::Origin;
    use frame_support::{
        dispatch::{GetDispatchInfo, PostDispatchInfo},
        pallet_prelude::*,
    };
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::Dispatchable;

    pub(crate) const SEED: [u8; 8] = *b"built/in";

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The overarching call type.
        type RuntimeCall: Parameter
            + Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
            + GetDispatchInfo;

        /// The builtin actor type.
        type Builtins: BuiltinCollection;

        /// Block limits.
        type BlockLimiter: BlockLimiter<Balance = u64>;

        /// Weight cost incurred by builtin actors calls.
        type WeightInfo: WeightInfo;
    }

    /// The pallet's storage version.
    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    impl<T: Config> Pallet<T> {
        /// Returns list of known builtins.
        ///
        /// This fn has some overhead, therefore it should be called only when necessary.
        pub fn list_builtins() -> Vec<T::AccountId>
        where
            T::AccountId: Origin,
        {
            BuiltinRegistry::<T>::new()
                .list()
                .into_iter()
                .map(Origin::cast)
                .collect()
        }

        /// Generate an `actor_id` given a builtin ID.
        ///
        ///
        /// This does computations, therefore we should seek to cache the value at the time of
        /// a builtin actor registration.
        pub fn generate_actor_id(builtin_id: u64) -> ActorId {
            hash((SEED, builtin_id).encode().as_slice()).into()
        }

        pub(crate) fn dispatch_call(
            origin: ActorId,
            call: CallOf<T>,
            context: &mut BuiltinContext,
        ) -> Result<(), BuiltinActorError>
        where
            T::AccountId: Origin,
        {
            let call_info = call.get_dispatch_info();

            // Necessary upfront gas sufficiency checks
            let gas_cost = call_info.weight.ref_time();
            context.can_charge_gas(gas_cost)?;

            // Execute call
            let res = call.dispatch(frame_system::RawOrigin::Signed(origin.cast()).into());
            let actual_gas = extract_actual_weight(&res, &call_info).ref_time();

            // Now actually charge the gas
            context.try_charge_gas(actual_gas)?;

            res.inspect(|_| {
                log::debug!(
                    target: LOG_TARGET,
                    "Call dispatched successfully",
                );
            })
            .map(|_| ())
            .inspect_err(|e| {
                log::debug!(target: LOG_TARGET, "Error dispatching call: {e:?}");
            })
            .map_err(|e| BuiltinActorError::Custom(LimitedStr::from_small_str(e.into())))
        }
    }
}

impl<T: Config> BuiltinDispatcherFactory for Pallet<T> {
    type Context = BuiltinContext;
    type Result = BuiltinHandleResult;
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
    pub registry: BTreeMap<ActorId, (Box<ActorErrorHandleFn>, Box<WeightFn>)>,
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

    pub fn list(&self) -> Vec<ActorId> {
        self.registry.keys().copied().collect()
    }
}

impl<T: Config> BuiltinDispatcher for BuiltinRegistry<T> {
    type Context = BuiltinContext;
    type Result = BuiltinHandleResult;
    type Error = BuiltinActorError;

    fn lookup<'a>(
        &'a self,
        id: &ActorId,
    ) -> Option<BuiltinInfo<'a, Self::Context, Self::Result, Self::Error>> {
        self.registry.get(id).map(|(f, g)| BuiltinInfo::<
            'a,
            Self::Context,
            Self::Result,
            Self::Error,
        > {
            handle: &**f,
            max_gas: &**g,
        })
    }

    fn run(
        &self,
        context: BuiltinInfo<Self::Context, Self::Result, Self::Error>,
        dispatch: StoredDispatch,
        gas_limit: u64,
    ) -> Vec<JournalNote> {
        let actor_id = dispatch.destination();

        let BuiltinInfo { handle, max_gas } = context;

        if dispatch.kind() != DispatchKind::Handle {
            let err_msg = format!(
                "BuiltinRegistry::run: Only handle dispatches can end up here. \
                Dispatch kind - {dispatch_kind:?}",
                dispatch_kind = dispatch.kind()
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        }
        if dispatch.context().is_some() {
            unreachable!(
                "BuiltinRegistry::run: Builtin actors can't have context from earlier executions"
            );
        }

        // We only allow a message to even start processing if it can fit into the current block
        // TODO: use fine-grained `max_gas` estimation based on payload info (#4395)
        let current_gas_allowance = GasAllowanceOf::<T>::get();
        if max_gas() > current_gas_allowance {
            return process_allowance_exceed(dispatch.into_incoming(gas_limit), actor_id, 0);
        }

        // Setting up the context to track gas usage.
        let mut context = BuiltinContext {
            gas_counter: GasCounter::new(gas_limit),
            gas_allowance_counter: GasAllowanceCounter::new(current_gas_allowance),
        };

        // Actual call to the builtin actor
        let res = handle(&dispatch, &mut context);

        let dispatch = dispatch.into_incoming(gas_limit);

        // Consume the context and extract the amount of gas spent.
        let gas_amount = context.to_gas_amount();

        match res {
            Ok(handle_result) => {
                // Builtin actor call was successful and returned some payload.
                log::debug!(target: LOG_TARGET, "Builtin call dispatched successfully");

                let mut dispatch_result =
                    DispatchResult::success(dispatch.clone(), actor_id, gas_amount);

                debug_assert!(dispatch.value() >= handle_result.used_value,
                    "BuiltinRegistry::run: Dispatch value should be greater than or equal to used value"
                );

                let unused_value = dispatch
                    .value()
                    .checked_sub(handle_result.used_value)
                    .unwrap_or_else(|| {
                        unreachable!(
                            "BuiltinRegistry::run: Used value exceeds the value of the dispatch"
                        );
                    });

                // Create an artificial `MessageContext` object that will help us to generate
                // a reply from the builtin actor.
                let mut message_context =
                    MessageContext::new(dispatch, actor_id, Default::default());
                let packet = ReplyPacket::new(handle_result.payload, unused_value);

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
                    unreachable!("BuiltinRegistry::run: Failed to send reply from builtin actor");
                };

                // Using the core processor logic create necessary `JournalNote`'s for us.
                process_success(SuccessfulDispatchResultKind::Success, dispatch_result)
            }
            Err(BuiltinActorError::GasAllowanceExceeded) => {
                // Ideally, this should never happen, as we should have checked the gas allowance
                // before even entering the `handle` method. However, if this error does occur,
                // we should handle it by discarding the gas burned and requeuing the message.
                // N.B.: if `gas_amount.burned` is not zero, the cost is borne by the validator.
                process_allowance_exceed(dispatch, actor_id, 0)
            }
            Err(err) => {
                // Builtin actor call failed.
                log::debug!(target: LOG_TARGET, "Builtin actor error: {err:?}");
                let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);
                // The core processor will take care of creating necessary `JournalNote`'s.
                process_execution_error(
                    dispatch,
                    actor_id,
                    gas_amount.burned(),
                    system_reservation_ctx,
                    err,
                )
            }
        }
    }
}

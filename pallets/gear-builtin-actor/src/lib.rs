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
mod benchmarking;

mod migrations;

pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use alloc::string::ToString;
use core_processor::common::{DispatchOutcome, JournalNote};
use gear_core::{
    ids::{BuiltinId, MessageId, ProgramId},
    message::{ReplyMessage, ReplyPacket, StoredDispatch},
};
use gear_core_errors::SimpleExecutionError;
use impl_trait_for_tuples::impl_for_tuples;
pub use pallet_gear::BuiltinRouter;
use parity_scale_codec::{Decode, Encode};
use sp_io::hashing::blake2_256;
use sp_runtime::traits::{TrailingZeroInput, Zero};
use sp_std::prelude::*;
pub use weights::WeightInfo;

pub use pallet::*;

#[allow(dead_code)]
const LOG_TARGET: &str = "gear::builtin_actor";

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
pub trait BuiltinActor<Payload, Gas: Zero> {
    /// Handles a message and returns a result and the actual gas spent.
    fn handle(builtin_id: BuiltinId, payload: Payload) -> (BuiltinResult<Payload>, Gas);

    /// Returns the maximum gas cost that can be incurred by handling a message.
    fn max_gas_cost(builtin_id: BuiltinId) -> Gas;
}

pub trait RegisteredBuiltinActor<P, G: Zero>: BuiltinActor<P, G> {
    /// The global unique ID of the trait implementer type.
    const ID: BuiltinId;
}

// Assuming as many as 16 builtin actors for the meantime
#[impl_for_tuples(16)]
#[tuple_types_custom_trait_bound(RegisteredBuiltinActor<P, G>)]
impl<P, G: Zero> BuiltinActor<P, G> for Tuple {
    fn handle(builtin_id: BuiltinId, payload: P) -> (BuiltinResult<P>, G) {
        for_tuples!(
            #(
                if (Tuple::ID == builtin_id) {
                    return Tuple::handle(builtin_id, payload);
                }
            )*
        );
        (Err(BuiltinActorError::UnknownBuiltinId), Zero::zero())
    }

    fn max_gas_cost(builtin_id: BuiltinId) -> G {
        for_tuples!(
            #(
                if (Tuple::ID == builtin_id) {
                    return Tuple::max_gas_cost(builtin_id);
                }
            )*
        );
        Zero::zero()
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, traits::Get, PalletId};
    use frame_system::pallet_prelude::*;

    /// The current storage version.
    const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The builtin actor type.
        type BuiltinActor: BuiltinActor<Vec<u8>, u64>;

        /// Weight cost incurred by builtin actors calls.
        type WeightInfo: WeightInfo;

        /// The builtin actor pallet id, used for deriving unique actors ids.
        #[pallet::constant]
        type PalletId: Get<PalletId>;
    }

    /// Builtin actors program ids to builtin ids mapping.
    #[pallet::storage]
    #[pallet::getter(fn actors)]
    pub type Actors<T> = StorageMap<_, Identity, ProgramId, BuiltinId>;

    /// Errors for the gear-builtin-actor pallet.
    #[pallet::error]
    pub enum Error<T> {
        /// `BuiltinId` already existd.
        BuiltinIdAlreadyExists,
    }

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        pub builtin_ids: Vec<BuiltinId>,
        #[serde(skip)]
        pub _phantom: sp_std::marker::PhantomData<T>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            self.builtin_ids.iter().cloned().for_each(|id| {
                let actor_id = Pallet::<T>::generate_actor_id(id);
                Actors::<T>::insert(actor_id, id);
            });
        }
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

        /// Register a builtin actor.
        ///
        /// This function is supposed to be called during the Runtime upgrade to update the
        /// builtin actors cache (if new actors are being added).
        #[allow(unused)]
        pub(crate) fn register_actor<B, P, G>() -> DispatchResult
        where
            B: RegisteredBuiltinActor<P, G>,
            G: Zero,
        {
            let builtin_id = <B as RegisteredBuiltinActor<P, G>>::ID;
            let actor_id = Self::generate_actor_id(builtin_id);
            ensure!(
                !Actors::<T>::contains_key(actor_id),
                Error::<T>::BuiltinIdAlreadyExists
            );
            Actors::<T>::insert(actor_id, builtin_id);
            Ok(())
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
                amount: <T as Config>::WeightInfo::base_handle_weight()
                    .ref_time()
                    .saturating_add(gas_spent),
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
            err: BuiltinActorError,
        ) -> Vec<JournalNote> {
            let message_id = dispatch.id();
            let origin = dispatch.source();
            let actor_id = dispatch.destination();

            let mut journal = vec![];

            // No call dispatced, so no gas burned except for the base
            journal.push(JournalNote::GasBurned {
                message_id,
                amount: <T as Config>::WeightInfo::base_handle_weight().ref_time(),
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

impl<T: Config> BuiltinRouter<ProgramId> for Pallet<T> {
    type Dispatch = StoredDispatch;
    type Output = JournalNote;

    fn lookup(id: &ProgramId) -> Option<BuiltinId> {
        Self::actors(id)
    }

    fn dispatch(
        builtin_id: BuiltinId,
        dispatch: StoredDispatch,
        gas_limit: u64,
    ) -> Vec<Self::Output> {
        // Estimate maximum gas that can be spent during message processing.
        // The exact gas cost may depend on the payload and be only available postfactum.
        let max_gas = <T as Config>::BuiltinActor::max_gas_cost(builtin_id);
        if max_gas > gas_limit {
            return Self::process_error(&dispatch, BuiltinActorError::InsufficientGas);
        }

        let payload = dispatch.payload_bytes().to_vec();

        // Do message processing
        let (res, gas_spent) = <T as Config>::BuiltinActor::handle(builtin_id, payload);
        res.map_or_else(
            |err| {
                log::debug!(target: LOG_TARGET, "Builtin actor error: {:?}", err);
                Self::process_error(&dispatch, err)
            },
            |response_bytes| {
                log::debug!(target: LOG_TARGET, "Builtin call dispatched successfully");
                Self::process_success(&dispatch, gas_spent, response_bytes)
            },
        )
    }

    fn estimate_gas(builtin_id: BuiltinId) -> u64 {
        <T as Config>::BuiltinActor::max_gas_cost(builtin_id)
    }
}

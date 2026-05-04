// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::{
    AuthoritySetHash, ClearTimer, Config, Error, Event, Initialized, MessageNonce, Pallet, Paused,
    Queue, QueueCapacityOf, QueueChanged, QueueId, QueueMerkleRoot, QueueOverflowedSince,
    QueuesInfo,
};
use bp_header_chain::{
    AuthoritySet,
    justification::{self, GrandpaJustification},
};
use builtins_common::eth_bridge;
use common::Origin;
use frame_support::{Blake2_256, StorageHasher, ensure, traits::Get, weights::Weight};
use frame_system::pallet_prelude::{BlockNumberFor, HeaderFor};
use gprimitives::{ActorId, H160, H256, U256};
use pallet_gear_eth_bridge_primitives::EthMessage;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::{
    DispatchError, RuntimeAppPublic as _,
    traits::{Hash, Keccak256},
};
use sp_std::vec::Vec;

type FinalityProofOf<T> = FinalityProof<HeaderFor<T>>;
type GrandpaJustificationOf<T> = GrandpaJustification<HeaderFor<T>>;

/// NOTE: copy-pasted from `sc-consensus-grandpa` due to std-compatibility issues.
#[derive(Debug, PartialEq, Encode, Decode, Clone)]
pub struct FinalityProof<Header: sp_runtime::traits::Header> {
    /// The hash of block F for which justification is provided.
    pub block: Header::Hash,
    /// Justification of the block F.
    pub justification: Vec<u8>,
    /// The set of headers in the range (B; F] that we believe are unknown to the caller. Ordered.
    pub unknown_headers: Vec<Header>,
}

impl<T: Config> Pallet<T> {
    #[cfg(not(test))]
    pub(super) fn reset_overflowed_queue_impl(
        encoded_finality_proof: Vec<u8>,
    ) -> Result<(), Error<T>> {
        let Some(overflowed_since) = QueueOverflowedSince::<T>::get() else {
            return Err(Error::<T>::InvalidQueueReset);
        };

        let finalized_number = Self::verify_finality_proof(encoded_finality_proof)
            .ok_or(Error::<T>::InvalidQueueReset)?;

        ensure!(
            finalized_number >= overflowed_since,
            Error::<T>::InvalidQueueReset
        );

        log::debug!(
            "Resetting queue that is overflowed since {:?}, current block is {:?}, received info about finalization of {:?}",
            overflowed_since,
            <frame_system::Pallet<T>>::block_number(),
            finalized_number
        );

        Self::reset_queue();

        Ok(())
    }

    #[cfg(test)]
    pub(super) fn reset_overflowed_queue_impl(
        encoded_finality_proof: Vec<u8>,
    ) -> Result<(), Error<T>> {
        ensure!(
            QueueOverflowedSince::<T>::get().is_some(),
            Error::<T>::InvalidQueueReset
        );

        ensure!(
            encoded_finality_proof == vec![42u8],
            Error::<T>::InvalidQueueReset
        );

        Self::reset_queue();

        Ok(())
    }

    /// Verifies given finality proof for actual grandpa set.
    ///
    /// Returns latest known finalized block number on success.
    ///
    /// See `FinalityProof` above.
    pub fn verify_finality_proof(encoded_finality_proof: Vec<u8>) -> Option<BlockNumberFor<T>> {
        // Decoding finality proof.
        let finality_proof = FinalityProofOf::<T>::decode(&mut encoded_finality_proof.as_ref())
            .inspect_err(|_| log::debug!("verify finality error: proof decoding"))
            .ok()?;

        // Extracting justification from the proof.
        let mut justification =
            GrandpaJustificationOf::<T>::decode(&mut finality_proof.justification.as_ref())
                .inspect_err(|_| log::debug!("verify finality error: justification decoding"))
                .ok()?;

        // Extracting finalized target from the justification.
        let finalized_target = (
            justification.commit.target_hash,
            justification.commit.target_number,
        );

        // Actual authorities and their set id.
        let authorities = <pallet_grandpa::Pallet<T>>::grandpa_authorities();
        let set_id = <pallet_grandpa::Pallet<T>>::current_set_id();

        let authority_set = AuthoritySet::new(authorities, set_id);
        let context = authority_set
            .try_into()
            .inspect_err(|_| log::debug!("verify finality error: invalid authority list"))
            .ok()?;

        // Verification of the finality.
        justification::verify_and_optimize_justification(
            finalized_target,
            &context,
            &mut justification,
        )
        .inspect_err(|e| {
            use bp_header_chain::justification::{
                JustificationVerificationError::*, PrecommitError::*,
            };

            log::debug!(
                "verify finality error: verification ({})",
                match e {
                    InvalidAuthorityList => "invalid authority list",
                    InvalidJustificationTarget => "invalid justification target",
                    DuplicateVotesAncestries => "duplicate votes ancestries",
                    Precommit(e) => match e {
                        RedundantAuthorityVote => "precommit: redundant authority vote",
                        UnknownAuthorityVote => "precommit: unknown authority vote",
                        DuplicateAuthorityVote => "precommit: duplicate authority vote",
                        InvalidAuthoritySignature => "precommit: invalid authority signature",
                        UnrelatedAncestryVote => "precommit: unrelated ancestry vote",
                    },
                    TooLowCumulativeWeight => "too low cumulative weight",
                    RedundantVotesAncestries => "redundant votes ancestries",
                }
            );
        })
        .ok()?;

        Some(justification.commit.target_number)
    }

    /// Updates the authority set hash in storage and emits an event.
    pub(super) fn update_authority_set_hash<'a, I>(validators: I)
    where
        I: Iterator<Item = (&'a T::AccountId, sp_consensus_grandpa::AuthorityId)>,
    {
        log::debug!("Updating the authority set hash");

        // Collecting all keys into `Vec<u8>`.
        let keys_bytes = validators
            .flat_map(|(_, key)| key.to_raw_vec())
            .collect::<Vec<_>>();

        // Hashing keys bytes with `Blake2`.
        let grandpa_set_hash = Blake2_256::hash(&keys_bytes).into();

        // Setting new grandpa set hash into storage.
        AuthoritySetHash::<T>::put(grandpa_set_hash);

        // Depositing event about update in the set.
        Self::deposit_event(Event::<T>::AuthoritySetHashChanged(grandpa_set_hash));
    }

    /// Updates the queue merkle root in storage and emits an event if it has changed.
    pub(super) fn update_queue_merkle_root_if_changed() {
        if !QueueChanged::<T>::take() {
            return;
        }

        log::debug!("Updating the queue merkle root");

        // Querying actual queue.
        let queue = Queue::<T>::get();
        let queue_len = queue.len();

        match queue_len {
            0 => {
                log::error!("Queue were changed within the block, but it's empty");
                return;
            }
            // If we reached queue capacity, it's time to reset the queue,
            // so it could handle further messages with next queue id.
            x if x >= QueueCapacityOf::<T>::get() as usize => {
                QueueOverflowedSince::<T>::put(<frame_system::Pallet<T>>::block_number());

                Self::deposit_event(Event::<T>::QueueOverflowed);
            }
            _ => {}
        }

        // Calculating new root.
        let root = binary_merkle_tree::merkle_root_raw::<Keccak256, _>(queue);

        // Updating queue merkle root in storage.
        QueueMerkleRoot::<T>::put(root);

        // Querying current queue id.
        let queue_id = QueueId::<T>::get();

        // Calculating last nonce used as `next_message_nonce - 1`, since queue is not empty,
        // so it was bumped within the block.
        let latest_nonce_used = MessageNonce::<T>::get().saturating_sub(U256::one());

        // Updating queue info in storage.
        QueuesInfo::<T>::insert(
            queue_id,
            QueueInfo::NonEmpty {
                highest_root: root,
                latest_nonce_used,
            },
        );

        // Depositing event about queue root being updated.
        Self::deposit_event(Event::<T>::QueueMerkleRootChanged { queue_id, root });
    }

    /// Clears the bridge state: removes timer, authority set hash, message queue and emits event.
    pub(super) fn clear_bridge() -> Weight {
        log::debug!("Clearing the bridge state");

        let mut weight = Weight::zero();
        let db_weight = T::DbWeight::get();

        // Removing timer for the next session hook.
        ClearTimer::<T>::kill();
        weight = weight.saturating_add(db_weight.writes(1));

        // Resetting queue.
        let reset_weight = Self::reset_queue();
        weight = weight.saturating_add(reset_weight);

        weight
    }

    /// Resets the message queue, it's merkle root and bumps queue id.
    pub(super) fn reset_queue() -> Weight {
        log::debug!("Resetting the message queue");

        // Removing queued messages from storage.
        Queue::<T>::kill();

        // Removing overflowed since block.
        QueueOverflowedSince::<T>::kill();

        // Bumping queue id for future use.
        let new_queue_id = QueueId::<T>::mutate(|id| {
            *id = id.saturating_add(1);
            *id
        });

        // Setting info for future queue.
        QueuesInfo::<T>::insert(new_queue_id, QueueInfo::Empty);

        // Setting zero queue root, keeping invariant of this key existence.
        QueueMerkleRoot::<T>::put(H256::zero());

        // Depositing event about queue being reset.
        Self::deposit_event(Event::<T>::QueueReset);

        T::DbWeight::get().writes(5)
    }

    pub(crate) fn queue_message(
        source: ActorId,
        destination: H160,
        payload: Vec<u8>,
    ) -> Result<(U256, H256), Error<T>>
    where
        T::AccountId: Origin,
    {
        // Ensuring that pallet is initialized.
        ensure!(
            Initialized::<T>::get(),
            Error::<T>::BridgeIsNotYetInitialized
        );

        let from_governance = Self::ensure_admin_or_pauser(source).is_ok();

        // Ensuring that pallet isn't paused if it's not forced from governance.
        if !from_governance {
            ensure!(!Paused::<T>::get(), Error::<T>::BridgeIsPaused);
        }

        let (message, hash) = Queue::<T>::mutate(|queue| {
            // Ensuring that queue isn't full if it's not forced from governance.
            if !from_governance && queue.len() >= QueueCapacityOf::<T>::get() as usize {
                return Err(Error::<T>::BridgeCleanupRequired);
            }

            // Creating new message from given data.
            //
            // Inside goes query and bump of nonce,
            // as well as checking payload size.
            let message = EthMessage::try_new(source, destination, payload)?;
            let hash = message.hash();

            // Appending hash of the message into the queue.
            queue.push(hash);

            // Marking queue as changed, so root will be updated later.
            QueueChanged::<T>::put(true);

            Ok((message, hash))
        })?;

        // Extracting nonce to return.
        let nonce = message.nonce();

        // Depositing event about message being queued for bridging.
        Self::deposit_event(Event::<T>::MessageQueued { message, hash });

        Ok((nonce, hash))
    }

    pub(super) fn ensure_admin_or_pauser(source: ActorId) -> Result<(), DispatchError>
    where
        T::AccountId: Origin,
    {
        let governance_addrs = [T::BridgeAdmin::get(), T::BridgePauser::get()];
        ensure!(
            governance_addrs.contains(&source.cast()),
            DispatchError::BadOrigin,
        );
        Ok(())
    }
}

/// Information about a specific message queue with its unique ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum QueueInfo {
    /// The queue is empty.
    Empty,
    /// The queue contains some messages, so it has a non-zero root.
    NonEmpty {
        /// The highest root that includes all messages in the queue.
        highest_root: H256,
        /// The latest message nonce used in the queue.
        ///
        /// Helps to identify the range of nonces to which the messages in the queue belong.
        latest_nonce_used: U256,
    },
}

/// Extension trait for [`EthMessage`] that provides additional functionality.
pub trait EthMessageExt: Sized {
    fn try_new<T: Config>(
        source: ActorId,
        destination: H160,
        payload: Vec<u8>,
    ) -> Result<Self, Error<T>>;

    fn hash(&self) -> H256;
}

impl EthMessageExt for EthMessage {
    /// Creates a new [`EthMessage`] with the given parameters.
    fn try_new<T: Config>(
        source: ActorId,
        destination: H160,
        payload: Vec<u8>,
    ) -> Result<Self, Error<T>> {
        ensure!(
            payload.len() <= T::MaxPayloadSize::get() as usize,
            Error::<T>::MaxPayloadSizeExceeded
        );

        let nonce = MessageNonce::<T>::mutate(|nonce| {
            let res = *nonce;
            *nonce = nonce.saturating_add(U256::one());
            res
        });

        Ok(unsafe { Self::new_unchecked(nonce, source, destination, payload) })
    }

    /// Returns hash of the message using `Keccak256` hasher.
    fn hash(&self) -> H256 {
        eth_bridge::bridge_call_hash(
            self.nonce(),
            self.source(),
            self.destination(),
            self.payload(),
            Keccak256::hash,
        )
    }
}

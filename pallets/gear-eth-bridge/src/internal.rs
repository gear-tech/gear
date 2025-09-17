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
    Queue, QueueCapacityOf, QueueChanged, QueueId, QueueMerkleRoot, QueuesInfo, ResetQueueOnInit,
};
use common::Origin;
use frame_support::{Blake2_256, StorageHasher, ensure, traits::Get, weights::Weight};
use gprimitives::{ActorId, H160, H256, U256};
use pallet_gear_eth_bridge_primitives::EthMessage;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::{
    DispatchError, RuntimeAppPublic as _,
    traits::{Hash, Keccak256},
};
use sp_std::vec::Vec;

impl<T: Config> Pallet<T> {
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
                log::debug!("Queue reached it's capacity. Scheduling next block reset");
                ResetQueueOnInit::<T>::put(true);
            }
            _ => {}
        }

        if queue_len == 0 {
            log::error!("Queue were changed within the block, but it's empty");
            return;
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

        // Removing grandpa set hash from storage.
        AuthoritySetHash::<T>::kill();
        Self::deposit_event(Event::<T>::AuthoritySetReset);
        weight = weight.saturating_add(db_weight.writes(2));

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

        T::DbWeight::get().writes(4)
    }

    // TODO (breathx): return bn as well?
    pub(crate) fn queue_message(
        source: ActorId,
        destination: H160,
        payload: Vec<u8>,
    ) -> Result<(U256, H256), Error<T>> {
        // Ensuring that pallet is initialized.
        ensure!(
            Initialized::<T>::get(),
            Error::<T>::BridgeIsNotYetInitialized
        );

        // Ensuring that pallet isn't paused.
        ensure!(!Paused::<T>::get(), Error::<T>::BridgeIsPaused);

        // Creating new message from given data.
        //
        // Inside goes query and bump of nonce,
        // as well as checking payload size.
        let message = EthMessage::try_new(source, destination, payload)?;
        let hash = message.hash();

        // Appending hash of the message into the queue.
        Queue::<T>::mutate(|v| v.push(hash));

        // Marking queue as changed, so root will be updated later.
        QueueChanged::<T>::put(true);

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
        let mut nonce = [0; 32];
        self.nonce().to_big_endian(&mut nonce);

        let bytes = [
            nonce.as_ref(),
            self.source().as_bytes(),
            self.destination().as_bytes(),
            self.payload(),
        ]
        .concat();

        Keccak256::hash(&bytes)
    }
}

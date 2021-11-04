// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

//! Offchain worker of the gear pallet.
//!
//! Monitors on-chain events to see if any messages have been added to
//! or removed from the wait list within the block in consideration.
//!
//! There can be any number of insertions/removals of the same message
//! to/from the waitlist over the span of a single block.
//! The only invariant here is that every insertion has to come first and
//! be eventually paired up with a removal (either in current block or
//! some time in the future); furthermore, if a waitlisted message is
//! carried forward to the next block, the number of insertions by the
//! end of each block equals exactly the number of removals + 1.
//!
//!           |             |
//!      In --|-->          |
//!           |    blk N    |
//!           |-------------|
//!                . . .
//!           |-------------|
//!           |           --|--> Out   
//!      In --|-->          |
//!           | blk (N + k) |
//!           |-------------|
//!                . . .
//!           |-------------|
//!           |           --|--> Out   
//!      In --|-->          |
//!           |           --|--> Out
//!           | blk (N + m) |
//!
//! Since the precise order of insertions/removals may not always be established,
//! it is possible to encounter two insertions or removals in a row.
//! Such situation should indicate that an opposite event "in between"
//! is yet to be revealed and accounted for (within the same block).
//!
//! In the context of charging a fee for "renting" a slot in the wait list, in case
//! a message is inserted to the list as many times it is removed from it (withing
//! one block) it is considered as having no effect on the wait list state.

use super::*;
use common::Message;
use primitive_types::H256;

use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, Encode};
use frame_support::sp_runtime::offchain::storage::StorageValueRef;
use frame_support::sp_runtime::offchain::storage_lock::{StorageLock, Time};
use frame_support::RuntimeDebug;
use frame_system::offchain::{SendUnsignedTransaction, SignedPayload, Signer, SigningTypes};
use frame_system::pallet_prelude::*;
use sp_runtime::traits::Saturating;
use sp_std::convert::TryInto;

// Off-chain worker constants
pub const STORAGE_OCW_WAITLIST: &'static [u8] = b"g::ocw::waitlist";
pub const STORAGE_OCW_LOCK: &'static [u8] = b"g::ocw::lock";

#[cfg_attr(test, derive(PartialEq))]
pub enum OffchainError<BlockNumber> {
    FailedToAcquireLock(BlockNumber),
    FailedSigning,
    FailedToGetValueFromStorage,
    SubmitTransaction,
    NoLocalAuthorityAvailable,
}

impl<BlockNumber> sp_std::fmt::Debug for OffchainError<BlockNumber>
where
    BlockNumber: sp_std::fmt::Debug,
{
    fn fmt(&self, fmt: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
        match *self {
            OffchainError::FailedToAcquireLock(ref deadline) => {
                write!(
                    fmt,
                    "the storage lock will not be released until after block {:?}.",
                    deadline
                )
            }
            OffchainError::FailedSigning => write!(fmt, "failed to sign transaction."),
            OffchainError::FailedToGetValueFromStorage => {
                write!(fmt, "failed to get value from storage.")
            }
            OffchainError::SubmitTransaction => write!(fmt, "failed to submit transaction."),
            OffchainError::NoLocalAuthorityAvailable => {
                write!(fmt, "No local authority has been found.")
            }
        }
    }
}

pub type OffchainResult<T, A> = Result<A, OffchainError<BlockNumberFor<T>>>;

// A shared offchain workers storage structure that maps a message ID onto
// the message itself, the number of block the message was added and an option
// which resolves into a block number if the message has been removed from the wait list
pub type WaitListTracker<BlockNumber> =
    BTreeMap<H256, (common::Message, BlockNumber, Option<BlockNumber>)>;

#[derive(Encode, Decode, Clone, PartialEq, RuntimeDebug, scale_info::TypeInfo)]
pub struct PaymentPayload<Public, BlockNumber> {
    pub block_number: BlockNumber,
    pub payment_data: Vec<WaitListInvoiceData<BlockNumber>>,
    public: Public,
}

impl<T: SigningTypes> SignedPayload<T> for PaymentPayload<T::Public, T::BlockNumber> {
    fn public(&self) -> T::Public {
        self.public.clone()
    }
}

#[derive(Encode, Decode, Default, Clone, PartialEq, RuntimeDebug, scale_info::TypeInfo)]
pub struct WaitListInvoiceData<BlockNumber> {
    pub program_id: H256,
    pub message_id: H256,
    pub start: BlockNumber,
    pub end: BlockNumber,
}

impl<T: Config> Pallet<T>
where
    T::AccountId: common::Origin,
{
    /// Iterate through the system events in current block and update
    /// the wait list tracking structure in the offchain storage.
    pub fn waitlist_usage(now: BlockNumberFor<T>) -> OffchainResult<T, ()> {
        // Acquire the lock protecting shared offchain workers' storage
        let mut lock = StorageLock::<'_, Time>::new(STORAGE_OCW_LOCK);
        let _guard = lock.lock();

        let storage_value_ref = StorageValueRef::persistent(STORAGE_OCW_WAITLIST);
        let mut waitlist_data = storage_value_ref
            .get::<WaitListTracker<T::BlockNumber>>()
            .map_err(|_| OffchainError::FailedToGetValueFromStorage)?
            .unwrap_or_default();

        // Count the incoming events: every insertion in the wait list increases the counter for
        // a message, every removal decreases it. The final balance: {-1 | 0 | 1} determines whether
        // the wait list tracker state should be updated.
        let mut current_events_counter: BTreeMap<H256, i32> = BTreeMap::new();
        let mut cached_messages: BTreeMap<H256, Message> = BTreeMap::new();

        // We can read the events here because offchain worker doesn't affect PoV.
        <frame_system::Pallet<T>>::read_events_no_consensus()
            .into_iter()
            .filter_map(|event_record| {
                <T as Config>::Event::from(event_record.event)
                    .try_into()
                    .ok()
            })
            .filter(|event| {
                matches!(
                    event,
                    Event::AddedToWaitList(_) | Event::RemovedFromWaitList(_)
                )
            })
            .for_each(|event| {
                let (msg_id, maybe_msg, score) = match event {
                    Event::AddedToWaitList(msg) => (msg.id, Some(msg), 1_i32),
                    Event::RemovedFromWaitList(msg_id) => (msg_id, None, -1_i32),
                    _ => unreachable!("only two types of events can be encountered here; qed"),
                };
                match current_events_counter.get_mut(&msg_id) {
                    Some(count) => *count = *count + score,
                    _ => {
                        current_events_counter.insert(msg_id, score);
                    }
                }
                if let Some(msg) = maybe_msg {
                    cached_messages.insert(msg_id, msg);
                }
            });
        log::debug!(
            target: "gear",
            "[waitlist_usage] After events processing in block {:?}. current_events_counter: {:?}, cached_messages: {:?}",
            now, current_events_counter, cached_messages,
        );

        // Updating the wait list tracking data structure according to the events
        // in current block (filtering out those that do not change wait list state)
        current_events_counter
            .into_iter()
            .filter(|(_, counter)| *counter != 0)
            .for_each(|(msg_id, counter)| match counter {
                -1_i32 => {
                    if let Some((_, _, maybe_removed_at)) = waitlist_data.get_mut(&msg_id) {
                        if maybe_removed_at.is_none() {
                            *maybe_removed_at = Some(now);
                        }
                    }
                }
                1_i32 => {
                    if let Some(msg) = cached_messages.get(&msg_id) {
                        waitlist_data.insert(msg_id, (msg.clone(), now, None));
                    }
                }
                _ => (),
            });

        storage_value_ref.set(&waitlist_data);

        if now % RENT_COLLECTION_INTERVAL.into() == 0_u32.into() {
            let billing_data = Self::prepare_invoice(now)?;
            return Self::send_transaction(now, billing_data);
        }

        Ok(())
    }

    // Iterate through the `waitlist_data` (usually, once every `RENT_COLLECTION_INTERVAL` blocks)
    // to collect the details of the programs that should be invoiced for using the wait list.
    fn prepare_invoice(
        now: T::BlockNumber,
    ) -> OffchainResult<T, Vec<WaitListInvoiceData<T::BlockNumber>>> {
        let storage_value_ref = StorageValueRef::persistent(STORAGE_OCW_WAITLIST);
        let mut waitlist_data = storage_value_ref
            .get::<WaitListTracker<T::BlockNumber>>()
            .map_err(|_| OffchainError::FailedToGetValueFromStorage)?
            .unwrap_or_default();

        log::debug!(
            target: "gear",
            "[prepare_invoice] Before processing block {:?}. waitlist_data: {:?}",
            now, waitlist_data,
        );

        // Billing senders of de-waitlisted messages
        let (removed_ids, mut billing_data): (Vec<H256>, Vec<WaitListInvoiceData<T::BlockNumber>>) =
            waitlist_data
                .iter()
                .filter_map(
                    |(msg_id, (msg, inserted_at, maybe_removed_at))| match maybe_removed_at {
                        Some(removed_at) => Some((
                            msg_id,
                            WaitListInvoiceData {
                                program_id: msg.dest,
                                message_id: msg.id,
                                start: *inserted_at,
                                end: *removed_at,
                            },
                        )),
                        _ => None,
                    },
                )
                .collect::<Vec<_>>()
                .into_iter()
                .unzip();
        log::debug!(
            target: "gear",
            "[prepare_invoice] After filtering delisted in block {:?}. removed_ids: {:?}, billing_data: {:?}, waitlist_data: {:?}",
            now, removed_ids, billing_data, waitlist_data,
        );
        for msg_id in removed_ids {
            waitlist_data.remove(&msg_id);
        }
        log::debug!(
            target: "gear",
            "[prepare_invoice] After removing delisted in block {:?}. billing_data: {:?}, waitlist_data: {:?}",
            now, billing_data, waitlist_data,
        );

        // Billing and requeueing messages that have stayed in the WL since last billing
        waitlist_data
            .iter_mut()
            .filter(|(_, (_, inserted_at, _))| {
                now.saturating_sub(*inserted_at) >= RENT_COLLECTION_INTERVAL.into()
            })
            .for_each(|(_, (msg, inserted_at, _))| {
                billing_data.push(WaitListInvoiceData {
                    program_id: msg.dest,
                    message_id: msg.id,
                    start: *inserted_at,
                    end: now,
                });
                *inserted_at = now;
            });
        log::debug!(
            target: "gear",
            "[prepare_invoice] After requeueing outstanding in block {:?}. billing_data: {:?}, waitlist_data: {:?}",
            now, billing_data, waitlist_data,
        );

        storage_value_ref.set(&waitlist_data);

        Ok(billing_data)
        // Self::send_transaction(now, billing_data)
    }

    fn send_transaction(
        block_number: T::BlockNumber,
        data: Vec<WaitListInvoiceData<T::BlockNumber>>,
    ) -> OffchainResult<T, ()> {
        let signer = Signer::<T, T::AuthorityId>::any_account();
        if !signer.can_sign() {
            log::error!("No local account available to sign offchain transaction");
            return Err(OffchainError::NoLocalAuthorityAvailable);
        }

        let (_, result) = signer
            .send_unsigned_transaction(
                |account| PaymentPayload {
                    block_number,
                    payment_data: data.clone(),
                    public: account.public.clone(),
                },
                |payload, signature| Call::collect_waitlist_rent { payload, signature },
            )
            .ok_or(OffchainError::NoLocalAuthorityAvailable)?;
        result.map_err(|()| OffchainError::SubmitTransaction)?;

        Ok(())
    }
}

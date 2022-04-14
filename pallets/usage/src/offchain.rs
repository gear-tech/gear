// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Offchain worker for the gear-support pallet.
//!
//! The offchain worker (OCW) of this pallet guarantees that the rent for using the on-chain
//! resources like the `WaitList` is collected at least once in a fixed number of blocks for
//! each message that has been occupying a slot in the wait list.
//!
//! Although the preferred mechanism for this would be incentivization of external players to
//! keep track of the wait list usage and sending signed extrinsics to charge those messages
//! that have been in the wait list longest, in exchange for a fraction of the collected fee,
//! we can't always rely that those external players will be continuously monitoring the wait
//! list usage.
//!
//! As a fallback mechanism, the OCW is run once at least every `T::WaitListTraversalInterval`
//! blocks. It scans the wait list storage top to bottom, keeping track of the latest checked
//! message and sends a transacton back on-chai with at most `T::MaxBatchSize` message ID's,
//! thus making sure the extrinsic doesn't take too much of the block weight.
//!
//! In case the wait list contains a lot of messages so that not all of them are visited within
//! the `T::WaitListTraversalInterval` blocks timespan, the scanning cycle duration naturally
//! streches until the entire list has been scanned. A new round will start immediately thereafter.
//!
//! An ordinary ("unstretched") timeline is as follows:
//!
//! ```ignore
//!
//!   block 0    |     1    |     2    |    3     |    4     |    5     |    6
//!   +----------+----------+----------+----------+----------+----------+-------
//!
//!              <---------- Min wait list traversal interval ---------->
//!
//!            +-----------------------------------+
//!   wait list  ||||||||
//!            +-^------^--------------------------+
//!               batch 1
//!
//!            +-----------------------------------+
//!   wait list             ||||||||
//!            +------------^------^---------------+
//!                          batch 2
//!
//!            +-----------------------------------+
//!   wait list                        ||||||||
//!            +-----------------------^------^----+
//!                                     batch 3
//!
//!                                                <------- Idle ------->
//!
//!                                                                         New round of invoicing
//!                                                                       +-----------------------------------+
//!                                                                         ||||||||
//!                                                                       +-^------^--------------------------+
//!                                                                          batch 1
//! ```
//!
//! For a "stretched" timeline, one round of full wait list scan can spill over the minimum
//! traversal interval thereby increasing the number of blocks between "invoices" to each
//! individual message.

use super::*;
use primitive_types::H256;

use alloc::vec::Vec;
use codec::{Decode, Encode};
use common::Origin;
use frame_support::{traits::Get, RuntimeDebug};
use frame_system::offchain::SubmitTransaction;
use frame_system::pallet_prelude::*;
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::offchain::storage::StorageValueRef;

// Off-chain worker constants
pub const STORAGE_LAST_KEY: &[u8] = b"g::ocw::last::key";
pub const STORAGE_OCW_LOCK: &[u8] = b"g::ocw::lock";
pub const STORAGE_ROUND_STARTED_AT: &[u8] = b"g::ocw::new::round";

#[cfg_attr(test, derive(PartialEq))]
pub enum OffchainError {
    FailedToGetValueFromStorage,
    SubmitTransaction,
}

impl sp_std::fmt::Debug for OffchainError {
    fn fmt(&self, fmt: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
        match *self {
            OffchainError::FailedToGetValueFromStorage => {
                write!(fmt, "failed to get value from storage.")
            }
            OffchainError::SubmitTransaction => write!(fmt, "failed to submit transaction."),
        }
    }
}

#[derive(Encode, Decode, Clone, PartialEq, scale_info::TypeInfo)]
pub struct PayeeInfo {
    pub program_id: H256,
    pub message_id: H256,
}

impl core::fmt::Debug for PayeeInfo {
    fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
        write!(
            f,
            "PayeeInfo {{ program_id: 0x{}…, message_id: 0x{}… }}",
            HexDisplay::from(&self.program_id[..4].to_vec()),
            HexDisplay::from(&self.message_id[..4].to_vec())
        )
    }
}

#[derive(Encode, Decode, Default, Clone, PartialEq, RuntimeDebug, scale_info::TypeInfo)]
pub struct WaitListInvoiceData<BlockNumber> {
    pub program_id: H256,
    pub message_id: H256,
    pub start: BlockNumber,
    pub end: BlockNumber,
}

type WaitListKeyIterator = frame_support::storage::KeyPrefixIterator<(H256, H256)>;

impl<T: Config> Pallet<T>
where
    T::AccountId: Origin,
{
    /// Iterates through a portion of the wait list and sends an unsigned transaction
    /// back on-chain to collect payment from the visited messages.
    pub fn waitlist_usage(now: BlockNumberFor<T>) -> Result<(), OffchainError> {
        let (storage_value_ref, last_key) = get_last_key_from_offchain_storage()?;
        let prefix = common::STORAGE_WAITLIST_PREFIX.to_vec();

        let mut iter = WaitListKeyIterator::new(prefix.clone(), last_key, |key| {
            // Each wait list item's key is a concatenation of three fragments:
            // `program_id` + "::" + `message_id`; The first and the third fragments are
            // 256 bits long, hence the total length of a correct key length is always 66 bytes
            assert_eq!(key.len(), 66);
            let prog_id = H256::from_slice(&key[0..32]);
            let msg_id = H256::from_slice(&key[34..]);
            Ok((prog_id, msg_id))
        });

        let mut entries = vec![];
        let mut counter = 0_u32;
        let mut new_last_key: Option<Vec<u8>>;
        // Iterate through the wait list storage starting from the entry following the `last_key`
        loop {
            new_last_key = iter.next().map(|(program_id, message_id)| {
                entries.push(PayeeInfo {
                    program_id,
                    message_id,
                });
                counter += 1;
                common::wait_key(program_id, message_id)
            });
            if new_last_key.is_none() || counter >= T::MaxBatchSize::get() {
                break;
            }
        }

        let new_last_key = new_last_key.unwrap_or(prefix);
        log::debug!(
            "Sending {} invoices to {:?} at block {:?}. Last visited key is 0x{}.",
            counter,
            entries,
            now,
            HexDisplay::from(&new_last_key),
        );

        storage_value_ref.set(&new_last_key);

        Self::send_transaction(entries)
    }

    fn send_transaction(data: Vec<PayeeInfo>) -> Result<(), OffchainError> {
        let call = Call::collect_waitlist_rent { payees_list: data };

        SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into()).map_err(|_| {
            log::debug!("Failure sending unsigned transaction");
            OffchainError::SubmitTransaction
        })
    }
}

pub fn get_last_key_from_offchain_storage<'a>(
) -> Result<(StorageValueRef<'a>, Vec<u8>), OffchainError> {
    let storage_value_ref = StorageValueRef::persistent(STORAGE_LAST_KEY);
    let last_key = storage_value_ref
        .get::<Vec<u8>>()
        .map_err(|_| OffchainError::FailedToGetValueFromStorage)?
        .unwrap_or_else(|| common::STORAGE_WAITLIST_PREFIX.to_vec());
    Ok((storage_value_ref, last_key))
}

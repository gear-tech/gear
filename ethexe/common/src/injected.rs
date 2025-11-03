// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{Address, ToDigest, db::OnChainStorageRO, ecdsa::SignedData};
use alloc::vec::Vec;
use anyhow::{Result, anyhow};
use core::hash::Hash;
use gprimitives::{ActorId, H256};
use parity_scale_codec::{Decode, Encode};
use sha3::Keccak256;

/// Recent block hashes window size used to check transaction mortality.
///
/// ### Rationale
/// The constant could have been defined in the `ethexe-db`,
/// but defined here to ease upgrades without invalidation of the transactions
/// stores.
pub const BLOCK_HASHES_WINDOW_SIZE: u32 = 32;

pub type SignedInjectedTransaction = SignedData<InjectedTransaction>;

#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq, Hash)]
pub struct InjectedTransaction {
    /// Address of validator the transaction intended for
    pub recipient: Address,
    /// Destination program inside `Gear.exe`.
    pub destination: ActorId,
    /// Payload of the message.
    pub payload: Vec<u8>,
    /// Value attached to the message.
    ///
    /// NOTE: at this moment will be zero.
    pub value: u128,
    /// Reference block number.
    pub reference_block: H256,
    /// Arbitrary bytes to allow multiple synonymous
    /// transactions to be sent simultaneously.
    ///
    /// NOTE: this is also a salt for MessageId generation.
    pub salt: Vec<u8>,
}

impl ToDigest for InjectedTransaction {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        let Self {
            recipient,
            destination,
            payload,
            value,
            reference_block,
            salt,
        } = self;

        recipient.0.update_hasher(hasher);
        destination.into_bytes().update_hasher(hasher);
        payload.update_hasher(hasher);
        value.to_be_bytes().update_hasher(hasher);
        reference_block.0.update_hasher(hasher);
        salt.update_hasher(hasher);
    }
}

#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct Promise {
    /// Payload of the reply.
    pub payload: Vec<u8>,
    /// Value attached to the reply.
    pub value: u128,
    // Reply code of the reply.
    // pub code: ReplyCode,
}

// TODO #4808: branch check must be until genesis block
/// Checks if the transaction is still valid at the given block.
/// Checking windows is in `transaction_height..transaction_height + BLOCK_HASHES_WINDOW_SIZE`
///
/// # Returns
/// - `true` if the transaction is still valid at the given block
/// - `false` otherwise
pub fn check_mortality_at<DB: OnChainStorageRO>(
    db: &DB,
    tx: &SignedInjectedTransaction,
    block_hash: H256,
) -> Result<bool> {
    let transaction_block_hash = tx.data().reference_block;
    let transaction_height = db
        .block_header(transaction_block_hash)
        .ok_or_else(|| {
            anyhow!("Block header not found for reference block {transaction_block_hash}")
        })?
        .height;

    let block_height = db
        .block_header(block_hash)
        .ok_or_else(|| anyhow!("Block header not found for hash: {block_hash}"))?
        .height;

    if transaction_height > block_height
        || transaction_height + BLOCK_HASHES_WINDOW_SIZE <= block_height
    {
        return Ok(false);
    }

    // Check transaction inclusion in the block branch.
    let mut block_hash = block_hash;
    for _ in 0..BLOCK_HASHES_WINDOW_SIZE {
        if block_hash == transaction_block_hash {
            return Ok(true);
        }

        block_hash = db
            .block_header(block_hash)
            .ok_or_else(|| anyhow!("Block header not found for hash: {block_hash}"))?
            .parent_hash;
    }

    Ok(false)
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::{BlockHeader, ProtocolTimelines, ecdsa::PrivateKey};
//     use alloc::collections::BTreeMap;

//     #[derive(Default)]
//     struct MockDatabase {
//         block_headers: BTreeMap<H256, BlockHeader>,
//     }

//     impl OnChainStorageRO for MockDatabase {
//         fn block_header(&self, hash: H256) -> Option<BlockHeader> {
//             self.block_headers.get(&hash).cloned()
//         }

//         fn protocol_timelines(&self) -> Option<ProtocolTimelines> {
//             unimplemented!()
//         }

//         fn block_events(&self, _block_hash: H256) -> Option<Vec<crate::events::BlockEvent>> {
//             unimplemented!()
//         }

//         fn code_blob_info(&self, _code_id: gprimitives::CodeId) -> Option<crate::CodeBlobInfo> {
//             unimplemented!()
//         }

//         fn block_synced(&self, _block_hash: H256) -> bool {
//             unimplemented!()
//         }

//         fn validators(&self, _era_index: u64) -> Option<crate::ValidatorsVec> {
//             unimplemented!()
//         }
//     }

//     fn mock_tx(reference_block: H256) -> SignedOffchainTransaction {
//         let raw_tx = RawOffchainTransaction::SendMessage {
//             program_id: H160::random(),
//             payload: vec![1, 2, 3],
//         };
//         SignedOffchainTransaction::create(
//             PrivateKey::random(),
//             OffchainTransaction {
//                 raw: raw_tx,
//                 reference_block,
//             },
//         )
//         .unwrap()
//     }

//     fn generate_chain(db: &mut MockDatabase, start_height: u32, count: u32) {
//         assert_ne!(start_height, 0);
//         assert_ne!(count, 0);
//         assert!(start_height + count <= u8::MAX as u32);

//         let mut parent_hash = H256::zero();
//         for height in start_height..start_height + count {
//             let block_hash = H256::from([height as u8; 32]);
//             db.block_headers.insert(
//                 block_hash,
//                 BlockHeader {
//                     height,
//                     timestamp: height as u64 * 10,
//                     parent_hash,
//                 },
//             );
//             parent_hash = block_hash;
//         }
//     }

//     #[test]
//     fn test_check_mortality_at() {
//         let mut db = MockDatabase::default();
//         let w = BLOCK_HASHES_WINDOW_SIZE as u8;

//         generate_chain(&mut db, 10, (w * 3) as u32);

//         let tx = mock_tx(H256::from([5; 32]));
//         check_mortality_at(&db, &tx, H256::from([15; 32])).unwrap_err();

//         let tx = mock_tx(H256::from([10; 32]));
//         check_mortality_at(&db, &tx, H256::from([25; 32])).unwrap();
//         assert!(check_mortality_at(&db, &tx, H256::from([35; 32])).unwrap());

//         let tx = mock_tx(H256::from([10; 32]));
//         assert!(check_mortality_at(&db, &tx, H256::from([10 + w - 1; 32])).unwrap());
//         assert!(!check_mortality_at(&db, &tx, H256::from([10 + w; 32])).unwrap());
//         assert!(!check_mortality_at(&db, &tx, H256::from([10 + w * 3 - 1; 32])).unwrap());
//         check_mortality_at(&db, &tx, H256::from([10 + w * 3; 32])).unwrap_err();

//         let tx = mock_tx(H256::from([11; 32]));
//         assert!(check_mortality_at(&db, &tx, H256::from([11; 32])).unwrap());
//         assert!(!check_mortality_at(&db, &tx, H256::from([10; 32])).unwrap());
//         check_mortality_at(&db, &tx, H256::from([9; 32])).unwrap_err();
//     }
// }

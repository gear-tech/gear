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

//! ethexe tx pool types

use crate::{ToDigest, db::OnChainStorageRead, ecdsa::SignedData};
use alloc::vec::Vec;
use anyhow::{Result, anyhow};
use derive_more::{Debug, Display};
use gprimitives::{H160, H256};
use parity_scale_codec::{Decode, Encode};
use sha3::Digest as _;

pub type SignedOffchainTransaction = SignedData<OffchainTransaction>;

impl SignedOffchainTransaction {
    /// Ethexe transaction blake2b256 hash.
    pub fn tx_hash(&self) -> H256 {
        gear_core::utils::hash(&self.encode()).into()
    }

    /// Ethexe transaction reference block hash
    ///
    /// Reference block hash is used for a transaction mortality check.
    pub fn reference_block(&self) -> H256 {
        self.data().reference_block
    }
}

/// Ethexe offchain transaction with a reference block for mortality.
#[derive(Clone, Encode, Decode, PartialEq, Eq, Debug, Display)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[display("OffchainTransaction {{ raw: {raw}, reference_block: {reference_block} }}")]
pub struct OffchainTransaction {
    pub raw: RawOffchainTransaction,
    pub reference_block: H256,
}

impl OffchainTransaction {
    /// Recent block hashes window size used to check transaction mortality.
    ///
    /// ### Rationale
    /// The constant could have been defined in the `ethexe-db`,
    /// but defined here to ease upgrades without invalidation of the transactions
    /// stores.
    pub const BLOCK_HASHES_WINDOW_SIZE: u32 = 32;
}

impl ToDigest for OffchainTransaction {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self.encode());
    }
}

/// Raw ethexe offchain transaction.
///
/// A particular job to be processed without external specifics.
#[derive(Clone, Encode, Decode, PartialEq, Eq, Debug, Display)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum RawOffchainTransaction {
    #[display(
        "SendMessage {{ program_id: {program_id}, payload: {} }}",
        hex::encode(payload)
    )]
    SendMessage { program_id: H160, payload: Vec<u8> },
}

impl RawOffchainTransaction {
    /// Gets the program id of the transaction.
    pub fn program_id(&self) -> H160 {
        match self {
            RawOffchainTransaction::SendMessage { program_id, .. } => *program_id,
        }
    }

    /// Gets the payload of the transaction.
    pub fn payload(&self) -> &[u8] {
        match self {
            RawOffchainTransaction::SendMessage { payload, .. } => payload,
        }
    }
}

// TODO #4808: branch check must be until genesis block
/// Checks if the transaction is still valid at the given block.
/// Checking windows is in `transaction_height..transaction_height + BLOCK_HASHES_WINDOW_SIZE`
///
/// # Returns
/// - `true` if the transaction is still valid at the given block
/// - `false` otherwise
pub fn check_mortality_at(
    db: &impl OnChainStorageRead,
    tx: &SignedOffchainTransaction,
    block_hash: H256,
) -> Result<bool> {
    let transaction_block_hash = tx.reference_block();
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
        || transaction_height + OffchainTransaction::BLOCK_HASHES_WINDOW_SIZE <= block_height
    {
        return Ok(false);
    }

    // Check transaction inclusion in the block branch.
    let mut block_hash = block_hash;
    for _ in 0..OffchainTransaction::BLOCK_HASHES_WINDOW_SIZE {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlockHeader, ecdsa::PrivateKey};
    use alloc::collections::BTreeMap;

    #[derive(Default)]
    struct MockDatabase {
        block_headers: BTreeMap<H256, BlockHeader>,
    }

    impl OnChainStorageRead for MockDatabase {
        fn block_header(&self, hash: H256) -> Option<BlockHeader> {
            self.block_headers.get(&hash).cloned()
        }

        fn block_events(&self, _block_hash: H256) -> Option<Vec<crate::events::BlockEvent>> {
            unimplemented!()
        }

        fn code_blob_info(&self, _code_id: gprimitives::CodeId) -> Option<crate::CodeBlobInfo> {
            unimplemented!()
        }

        fn block_validators(
            &self,
            _block_hash: H256,
        ) -> Option<nonempty::NonEmpty<crate::Address>> {
            unimplemented!()
        }

        fn block_synced(&self, _block_hash: H256) -> bool {
            unimplemented!()
        }
    }

    fn mock_tx(reference_block: H256) -> SignedOffchainTransaction {
        let raw_tx = RawOffchainTransaction::SendMessage {
            program_id: H160::random(),
            payload: vec![1, 2, 3],
        };
        SignedOffchainTransaction::create(
            PrivateKey::random(),
            OffchainTransaction {
                raw: raw_tx,
                reference_block,
            },
        )
        .unwrap()
    }

    fn generate_chain(db: &mut MockDatabase, start_height: u32, count: u32) {
        assert_ne!(start_height, 0);
        assert_ne!(count, 0);
        assert!(start_height + count <= u8::MAX as u32);

        let mut parent_hash = H256::zero();
        for height in start_height..start_height + count {
            let block_hash = H256::from([height as u8; 32]);
            db.block_headers.insert(
                block_hash,
                BlockHeader {
                    height,
                    timestamp: height as u64 * 10,
                    parent_hash,
                },
            );
            parent_hash = block_hash;
        }
    }

    #[test]
    fn test_check_mortality_at() {
        let mut db = MockDatabase::default();
        let w = OffchainTransaction::BLOCK_HASHES_WINDOW_SIZE as u8;

        generate_chain(&mut db, 10, (w * 3) as u32);

        let tx = mock_tx(H256::from([5; 32]));
        check_mortality_at(&db, &tx, H256::from([15; 32])).unwrap_err();

        let tx = mock_tx(H256::from([10; 32]));
        check_mortality_at(&db, &tx, H256::from([25; 32])).unwrap();
        assert!(check_mortality_at(&db, &tx, H256::from([35; 32])).unwrap());

        let tx = mock_tx(H256::from([10; 32]));
        assert!(check_mortality_at(&db, &tx, H256::from([10 + w - 1; 32])).unwrap());
        assert!(!check_mortality_at(&db, &tx, H256::from([10 + w; 32])).unwrap());
        assert!(!check_mortality_at(&db, &tx, H256::from([10 + w * 3 - 1; 32])).unwrap());
        check_mortality_at(&db, &tx, H256::from([10 + w * 3; 32])).unwrap_err();

        let tx = mock_tx(H256::from([11; 32]));
        assert!(check_mortality_at(&db, &tx, H256::from([11; 32])).unwrap());
        assert!(!check_mortality_at(&db, &tx, H256::from([10; 32])).unwrap());
        check_mortality_at(&db, &tx, H256::from([9; 32])).unwrap_err();
    }
}

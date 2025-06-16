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

use crate::{BatchCommitmentValidationReply, BatchCommitmentValidationRequest};
use ethexe_common::{
    db::{BlockMetaStorageWrite, CodesStorageWrite, OnChainStorageRead, OnChainStorageWrite},
    ecdsa::{PrivateKey, PublicKey, SignedData},
    gear::{BlockCommitment, CodeCommitment, Message, StateTransition},
    Address, BlockHeader, CodeBlobInfo, Digest, ProducerBlock, SimpleBlockData,
};
use ethexe_db::Database;
use ethexe_signer::Signer;
use gprimitives::H256;
use std::vec;

pub fn init_signer_with_keys(amount: u8) -> (Signer, Vec<PrivateKey>, Vec<PublicKey>) {
    let signer = Signer::memory();

    let private_keys: Vec<_> = (0..amount).map(|i| PrivateKey::from([i + 1; 32])).collect();
    let public_keys = private_keys
        .iter()
        .map(|&key| signer.storage_mut().add_key(key).unwrap())
        .collect();
    (signer, private_keys, public_keys)
}

pub fn mock_simple_block_data() -> SimpleBlockData {
    let block_hash = H256::random();
    let parent_hash = H256::random();
    SimpleBlockData {
        hash: block_hash,
        header: BlockHeader {
            height: 43,
            timestamp: 120,
            parent_hash,
        },
    }
}

pub fn mock_producer_block(
    signer: &Signer,
    producer: PublicKey,
    block_hash: H256,
) -> (ProducerBlock, SignedData<ProducerBlock>) {
    let pb = ProducerBlock {
        block_hash,
        gas_allowance: Some(100),
        off_chain_transactions: vec![],
    };

    let signed_pb = signer.signed_data(producer, pb.clone()).unwrap();

    (pb, signed_pb)
}

pub fn mock_validation_request(
    signer: &Signer,
    public_key: PublicKey,
) -> (
    BatchCommitmentValidationRequest,
    SignedData<BatchCommitmentValidationRequest>,
) {
    let request = BatchCommitmentValidationRequest {
        blocks: vec![],
        codes: vec![mock_code_commitment(), mock_code_commitment()],
    };
    let signed = signer.signed_data(public_key, request.clone()).unwrap();
    (request, signed)
}

pub fn mock_validation_reply(
    signer: &Signer,
    public_key: PublicKey,
    contract_address: Address,
    digest: Digest,
) -> BatchCommitmentValidationReply {
    BatchCommitmentValidationReply {
        digest,
        signature: signer
            .sign_for_contract(contract_address, public_key, digest)
            .unwrap(),
    }
}

pub fn mock_code_commitment() -> CodeCommitment {
    CodeCommitment {
        id: H256::random().into(),
        timestamp: 123,
        valid: true,
    }
}

pub fn mock_state_transition() -> StateTransition {
    StateTransition {
        actor_id: H256::random().into(),
        new_state_hash: H256::random(),
        exited: true,
        inheritor: H256::random().into(),
        value_to_receive: 123,
        value_claims: vec![],
        messages: vec![Message {
            id: H256::random().into(),
            destination: H256::random().into(),
            payload: b"Hello, World!".to_vec(),
            value: 0,
            reply_details: None,
            call: false,
        }],
    }
}

pub fn mock_block_commitment(
    hash: H256,
    predecessor: H256,
    previous_not_empty: H256,
) -> (SimpleBlockData, BlockCommitment) {
    let mut block = mock_simple_block_data();
    block.hash = hash;

    let transitions = vec![mock_state_transition(), mock_state_transition()];

    (
        block.clone(),
        BlockCommitment {
            hash: block.hash,
            timestamp: block.header.timestamp,
            previous_committed_block: previous_not_empty,
            predecessor_block: predecessor,
            transitions,
        },
    )
}

pub fn prepare_code_commitment(db: &Database, code: CodeCommitment) -> CodeCommitment {
    db.set_code_blob_info(
        code.id,
        CodeBlobInfo {
            timestamp: code.timestamp,
            tx_hash: H256::random(),
        },
    );
    db.set_code_valid(code.id, code.valid);
    code
}

pub fn prepare_block_commitment(
    db: &Database,
    (block, commitment): (SimpleBlockData, BlockCommitment),
) -> (SimpleBlockData, BlockCommitment) {
    prepare_mock_empty_block(db, &block, commitment.previous_committed_block);

    db.set_block_outcome(block.hash, commitment.transitions.clone());

    if commitment.predecessor_block != block.hash
        && db.block_header(commitment.predecessor_block).is_none()
    {
        // If predecessor is not the same as block.hash, we need to set the block header
        // Set predecessor (note: it is predecessor of block where commitment would apply) as child of block
        db.set_block_header(
            commitment.predecessor_block,
            BlockHeader {
                height: block.header.height + 1,
                timestamp: block.header.timestamp + 1,
                parent_hash: block.hash,
            },
        );
    }

    (block, commitment)
}

pub fn prepare_mock_empty_block(
    db: &Database,
    block: &SimpleBlockData,
    previous_committed_block: H256,
) {
    db.set_block_computed(block.hash);
    db.set_block_header(block.hash, block.header.clone());
    db.set_previous_not_empty_block(block.hash, previous_committed_block);
    db.set_block_codes_queue(block.hash, Default::default());
    db.set_block_commitment_queue(block.hash, Default::default());
    db.set_block_outcome(block.hash, Default::default());
}

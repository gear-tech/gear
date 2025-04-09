use std::vec;

use crate::{BatchCommitmentValidationReply, BatchCommitmentValidationRequest};
use ethexe_common::{
    gear::{BlockCommitment, CodeCommitment, Message, StateTransition},
    ProducerBlock, SimpleBlockData,
};
use ethexe_db::{BlockHeader, BlockMetaStorage, CodeInfo, CodesStorage, Database, OnChainStorage};
use ethexe_signer::{Address, Digest, PrivateKey, PublicKey, SignedData, Signer};
use gprimitives::H256;

pub fn init_signer_with_keys(amount: u8) -> (Signer, Vec<PrivateKey>, Vec<PublicKey>) {
    let signer = Signer::tmp();
    let private_keys: Vec<_> = (0..amount).map(|i| PrivateKey([i + 1; 32])).collect();
    let public_keys = private_keys
        .iter()
        .map(|&key| signer.add_key(key).unwrap())
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

    let signed_pb = signer.create_signed_data(producer, pb.clone()).unwrap();

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
    let signed = signer
        .create_signed_data(public_key, request.clone())
        .unwrap();
    (request, signed)
}

#[allow(unused)]
pub fn mock_validation_reply(
    signer: &Signer,
    public_key: PublicKey,
    contract_address: Address,
) -> BatchCommitmentValidationReply {
    let digest: Digest = H256::random().0.into();
    BatchCommitmentValidationReply {
        digest,
        signature: signer
            .contract_signer(contract_address)
            .sign_digest(public_key, digest)
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
        inheritor: H256::random().into(),
        value_to_receive: 123,
        value_claims: vec![],
        messages: vec![Message {
            id: H256::random().into(),
            destination: H256::random().into(),
            payload: b"Hello, World!".to_vec(),
            value: 0,
            reply_details: None,
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
        CodeInfo {
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

    if commitment.predecessor_block != block.hash {
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

use crate::{ControlError, ControlEvent};
use anyhow::anyhow;
use ethexe_common::{
    db::{BlockMetaStorage, CodesStorage, OnChainStorage},
    gear::{BatchCommitment, BlockCommitment, CodeCommitment},
    ProducerBlock, SimpleBlockData,
};
use ethexe_db::Database;
use ethexe_signer::{Address, PublicKey, Signer};
use gprimitives::H256;

pub struct Producer {
    pub_key: PublicKey,
    signer: Signer,
    db: Database,
    validators: Vec<Address>,
    block: SimpleBlockData,
    state: ProducerState,
}

enum ProducerState {
    #[allow(unused)]
    CollectOffChainTransactions,
    WaitingBlockComputed(H256),
    Final,
}

impl Producer {
    pub fn new(
        pub_key: PublicKey,
        signer: Signer,
        db: Database,
        validators: Vec<Address>,
        block: SimpleBlockData,
    ) -> Result<(Self, Vec<ControlEvent>), anyhow::Error> {
        let block_hash = block.hash;

        let producer = Self {
            pub_key,
            signer,
            db,
            validators,
            block,
            // TODO +_+_+: collect off-chain transactions is skipped for now
            state: ProducerState::WaitingBlockComputed(block_hash),
        };

        let block = ProducerBlock {
            block_hash,
            // TODO +_+_+: set gas allowance here
            gas_allowance: Some(3_000_000_000_000),
            // TODO +_+_+: append off-chain transactions
            off_chain_transactions: Vec::new(),
        };

        let signed_block = producer
            .signer
            .create_signed_data(producer.pub_key, block)?;

        Ok((
            producer,
            vec![
                ControlEvent::ComputeProducerBlock(signed_block.data().clone()),
                ControlEvent::PublishProducerBlock(signed_block),
            ],
        ))
    }

    pub fn receive_computed_block(
        &mut self,
        computed_block: H256,
    ) -> Result<Option<BatchCommitment>, ControlError> {
        match &mut self.state {
            ProducerState::CollectOffChainTransactions => Err(ControlError::Fatal(anyhow!(
                "CollectOffChainTransactions is not supported"
            ))),
            ProducerState::WaitingBlockComputed(block_hash) => {
                if computed_block != *block_hash {
                    return Err(ControlError::Warning(anyhow!(
                        "Received computed block {} is different from the expected block hash {}",
                        computed_block,
                        block_hash
                    )));
                }

                self.state = ProducerState::Final;
                self.aggregate_commitments_for_block(computed_block)
                    .map_err(Into::into)
            }
            ProducerState::Final => Err(ControlError::Fatal(anyhow!(
                "Producer is in the final state"
            ))),
        }
    }

    pub fn into_parts(self) -> (Vec<Address>, SimpleBlockData) {
        if !matches!(self.state, ProducerState::Final) {
            unreachable!("Producer is not in the final state: wrong Producer usage");
        }

        (self.validators, self.block)
    }

    fn aggregate_commitments_for_block(
        &self,
        block_hash: H256,
    ) -> Result<Option<BatchCommitment>, anyhow::Error> {
        let block_commitments = match self.aggregate_block_commitments_for_block(block_hash) {
            Ok(commitments) => commitments,
            Err(BlocksAggregationError::SomeBlocksInQueueAreNotComputed) => {
                log::warn!("Some blocks in the queue are not computed for block {block_hash}");
                return Ok(None);
            }
            Err(BlocksAggregationError::Any(e)) => return Err(e),
        };

        let code_commitments = self.aggregate_code_commitments_for_block(block_hash)?;

        Ok(
            (!block_commitments.is_empty() || !code_commitments.is_empty()).then_some(
                BatchCommitment {
                    block_commitments,
                    code_commitments,
                },
            ),
        )
    }

    fn aggregate_block_commitments_for_block(
        &self,
        block_hash: H256,
    ) -> Result<Vec<BlockCommitment>, BlocksAggregationError> {
        let commitments_queue = self
            .db
            .block_commitment_queue(block_hash)
            .ok_or_else(|| anyhow!("Block {block_hash} commitment queue is not in storage"))?;

        let mut commitments = Vec::new();

        let predecessor_block = block_hash;

        for block in commitments_queue {
            if !self.db.block_computed(block) {
                // This can happen when validator syncs from p2p network and skips some old blocks.
                return Err(BlocksAggregationError::SomeBlocksInQueueAreNotComputed);
            }

            let outcomes = self
                .db
                .block_outcome(block)
                .ok_or_else(|| anyhow!("Cannot get from db outcome for computed block {block}"))?;

            let previous_committed_block =
                self.db.previous_not_empty_block(block).ok_or_else(|| {
                    anyhow!(
                        "Cannot get from db previous committed block for computed block {block}"
                    )
                })?;

            let header = self
                .db
                .block_header(block)
                .ok_or_else(|| anyhow!("Cannot get from db header for computed block {block}"))?;

            commitments.push(BlockCommitment {
                hash: block,
                timestamp: header.timestamp,
                previous_committed_block,
                predecessor_block,
                transitions: outcomes,
            });
        }

        Ok(commitments)
    }

    fn aggregate_code_commitments_for_block(
        &self,
        block_hash: H256,
    ) -> Result<Vec<CodeCommitment>, anyhow::Error> {
        Ok(self
            .db
            .block_codes_queue(block_hash)
            .ok_or_else(|| anyhow!("Cannot get from db codes queue for block {block_hash}"))?
            .into_iter()
            .filter_map(|code_id| {
                let Some(code_info) = self.db.code_blob_info(code_id) else {
                    // TODO +_+_+: this must be an error
                    return None;
                };
                self.db.code_valid(code_id).map(|valid| CodeCommitment {
                    id: code_id,
                    timestamp: code_info.timestamp,
                    valid,
                })
            })
            .collect())
    }
}

#[derive(Debug, derive_more::From)]
enum BlocksAggregationError {
    SomeBlocksInQueueAreNotComputed,
    #[from]
    Any(anyhow::Error),
}

#[cfg(test)]
mod tests {
    use std::vec;

    use ethexe_db::CodeInfo;

    use super::*;
    use crate::test_utils::*;

    #[test]
    fn producer_new() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);
        let pub_key = pub_keys[0];
        let db = Database::memory();
        let validators = vec![Address([1; 20]), Address([2; 20])];
        let block = mock_simple_block_data();

        let (producer, events) = Producer::new(
            pub_key,
            signer.clone(),
            db.clone(),
            validators.clone(),
            block.clone(),
        )
        .expect("Failed to create producer");

        assert_eq!(producer.pub_key, pub_key);
        assert_eq!(producer.validators, validators);
        assert_eq!(producer.block, block);
        assert!(matches!(
            producer.state,
            ProducerState::WaitingBlockComputed(hash) if hash == block.hash
        ));

        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[0],
            ControlEvent::ComputeProducerBlock(ref b) if b.block_hash == block.hash
        ));
        assert!(matches!(
            events[1],
            ControlEvent::PublishProducerBlock(ref signed) if signed.data().block_hash == block.hash
        ));
    }

    #[test]
    fn receive_wrong_computed_block() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);
        let pub_key = pub_keys[0];
        let db = Database::memory();
        let validators = vec![Address([1; 20])];
        let block = mock_simple_block_data();

        let (mut producer, _) =
            Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

        let wrong_block = H256::random();
        let result = producer.receive_computed_block(wrong_block);
        assert!(matches!(result, Err(ControlError::Warning(_))));
    }

    #[test]
    fn code_commitments_only() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);
        let pub_key = pub_keys[0];
        let db = Database::memory();
        let validators = vec![Address([1; 20])];
        let block = mock_simple_block_data();

        let (mut producer, _) =
            Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

        let code1 = prepare_mock_code_commitment(&db);
        let code2 = prepare_mock_code_commitment(&db);
        db.set_block_codes_queue(block.hash, [code1.id, code2.id].into_iter().collect());
        db.set_block_commitment_queue(block.hash, Default::default());
        let commitment = producer
            .receive_computed_block(block.hash)
            .unwrap()
            .unwrap();
        assert_eq!(commitment.block_commitments, vec![]);
        assert_eq!(commitment.code_commitments, vec![code1, code2]);
    }

    #[test]
    fn code_and_block_commitments() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);
        let pub_key = pub_keys[0];
        let db = Database::memory();
        let validators = vec![Address([1; 20])];

        let (block1_hash, block2_hash) = (H256::random(), H256::random());
        let (block1, block1_commitment) =
            prepare_mock_block_commitment(&db, block1_hash, block1_hash, block2_hash);
        let (block2, block2_commitment) =
            prepare_mock_block_commitment(&db, block2_hash, block1_hash, H256::random());

        let (mut producer, _) =
            Producer::new(pub_key, signer, db.clone(), validators, block1.clone()).unwrap();

        let code1 = prepare_mock_code_commitment(&db);
        let code2 = prepare_mock_code_commitment(&db);

        db.set_block_codes_queue(block1.hash, [code1.id, code2.id].into_iter().collect());
        db.set_block_commitment_queue(
            block1.hash,
            [block2.hash, block1.hash].into_iter().collect(),
        );

        let commitment = producer
            .receive_computed_block(block1.hash)
            .unwrap()
            .unwrap();
        assert_eq!(
            commitment.block_commitments,
            vec![block2_commitment, block1_commitment]
        );
        assert_eq!(commitment.code_commitments, vec![code1, code2]);
    }

    #[test]
    fn blocks_in_queue_not_computed() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);
        let pub_key = pub_keys[0];
        let db = Database::memory();
        let validators = vec![Address([1; 20])];
        let block = mock_simple_block_data();

        let (mut producer, _) =
            Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

        let (block1_hash, block2_hash) = (H256::random(), H256::random());
        let (block1, _) =
            prepare_mock_block_commitment(&db, block1_hash, block1_hash, block2_hash);

        // Simulate a block in the queue that is not computed
        db.set_block_commitment_queue(block.hash, [block1.hash, block2_hash].into_iter().collect());
        db.set_block_computed(block1.hash); // Only block1 is marked as computed

        let result = producer.receive_computed_block(block.hash);

        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn receive_computed_block_in_collect_off_chain_transactions_state() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);
        let pub_key = pub_keys[0];
        let db = Database::memory();
        let validators = vec![Address([1; 20])];
        let block = mock_simple_block_data();

        let (mut producer, _) =
            Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

        // Manually set the state to `CollectOffChainTransactions`
        producer.state = ProducerState::CollectOffChainTransactions;

        let computed_block = block.hash;
        let result = producer.receive_computed_block(computed_block);

        assert!(matches!(result, Err(ControlError::Fatal(_))));
    }

    #[test]
    fn receive_computed_block_with_wrong_hash() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);
        let pub_key = pub_keys[0];
        let db = Database::memory();
        let validators = vec![Address([1; 20])];
        let block = mock_simple_block_data();

        let (mut producer, _) =
            Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

        let wrong_block_hash = H256::random();
        let result = producer.receive_computed_block(wrong_block_hash);

        assert!(matches!(result, Err(ControlError::Warning(_))));
    }

    #[test]
    fn receive_computed_block_in_final_state() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);
        let pub_key = pub_keys[0];
        let db = Database::memory();
        let validators = vec![Address([1; 20])];
        let block = mock_simple_block_data();

        let (mut producer, _) =
            Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

        // Simulate the producer reaching the final state
        producer.state = ProducerState::Final;

        let computed_block = block.hash;
        let result = producer.receive_computed_block(computed_block);

        assert!(matches!(result, Err(ControlError::Fatal(_))));
    }

    #[test]
    fn receive_computed_block_with_missing_commitment_queue() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);
        let pub_key = pub_keys[0];
        let db = Database::memory();
        let validators = vec![Address([1; 20])];
        let block = mock_simple_block_data();

        let (mut producer, _) =
            Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

        // Simulate missing commitment queue in the database
        let computed_block = block.hash;
        let result = producer.receive_computed_block(computed_block);

        assert!(matches!(result, Err(ControlError::Fatal(_))));
    }

    #[test]
    fn receive_computed_block_with_missing_outcome() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);
        let pub_key = pub_keys[0];
        let db = Database::memory();
        let validators = vec![Address([1; 20])];
        let block = mock_simple_block_data();

        let (mut producer, _) =
            Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

        // Simulate a block in the queue but missing outcome
        let block1_hash = H256::random();
        db.set_block_commitment_queue(block.hash, [block1_hash].into_iter().collect());
        db.set_block_computed(block1_hash);

        let computed_block = block.hash;
        let result = producer.receive_computed_block(computed_block);

        assert!(matches!(result, Err(ControlError::Fatal(_))));
    }

    #[test]
    fn receive_computed_block_with_missing_previous_committed_block() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);
        let pub_key = pub_keys[0];
        let db = Database::memory();
        let validators = vec![Address([1; 20])];
        let block = mock_simple_block_data();

        let (mut producer, _) =
            Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

        // Simulate a block in the queue but missing previous committed block
        let block1_hash = H256::random();
        db.set_block_commitment_queue(block.hash, [block1_hash].into_iter().collect());
        db.set_block_computed(block1_hash);
        db.set_block_outcome(block1_hash, vec![mock_state_transition()]);

        let computed_block = block.hash;
        let result = producer.receive_computed_block(computed_block);

        assert!(matches!(result, Err(ControlError::Fatal(_))));
    }

    #[test]
    fn receive_computed_block_with_missing_header() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);
        let pub_key = pub_keys[0];
        let db = Database::memory();
        let validators = vec![Address([1; 20])];
        let block = mock_simple_block_data();

        let (mut producer, _) =
            Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

        // Simulate a block in the queue but missing header
        let block1_hash = H256::random();
        db.set_block_commitment_queue(block.hash, [block1_hash].into_iter().collect());
        db.set_block_computed(block1_hash);
        db.set_block_outcome(block1_hash, vec![mock_state_transition()]);
        db.set_previous_not_empty_block(block1_hash, H256::random());

        let computed_block = block.hash;
        let result = producer.receive_computed_block(computed_block);

        assert!(matches!(result, Err(ControlError::Fatal(_))));
    }

    #[test]
    fn into_parts_works() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);
        let pub_key = pub_keys[0];
        let db = Database::memory();
        let validators = vec![Address([1; 20]), Address([2; 20])];
        let block = mock_simple_block_data();

        let (mut producer, _) =
            Producer::new(pub_key, signer, db.clone(), validators.clone(), block.clone()).unwrap();

        // Simulate the producer reaching the final state
        producer.state = ProducerState::Final;

        let (returned_validators, returned_block) = producer.into_parts();

        assert_eq!(returned_validators, validators);
        assert_eq!(returned_block, block);
    }

    #[test]
    #[should_panic(expected = "Producer is not in the final state: wrong Producer usage")]
    fn into_parts_panics_if_not_final() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);
        let pub_key = pub_keys[0];
        let db = Database::memory();
        let validators = vec![Address([1; 20]), Address([2; 20])];
        let block = mock_simple_block_data();

        let (producer, _) =
            Producer::new(pub_key, signer, db.clone(), validators, block).unwrap();

        // Attempt to call into_parts without reaching the final state
        let _ = producer.into_parts();
    }

    fn prepare_mock_code_commitment(db: &Database) -> CodeCommitment {
        let code = mock_code_commitment();
        db.set_code_blob_info(
            code.id,
            CodeInfo {
                timestamp: code.timestamp,
                tx_hash: H256::random(),
            },
        );
        db.set_code_valid(code.id, code.valid);
        db.set_code_valid(code.id, code.valid);
        code
    }

    fn prepare_mock_block_commitment(
        db: &Database,
        hash: H256,
        predecessor: H256,
        previous_not_empty: H256,
    ) -> (SimpleBlockData, BlockCommitment) {
        let mut block = mock_simple_block_data();
        block.hash = hash;

        let transitions = vec![mock_state_transition(), mock_state_transition()];
        db.set_block_computed(block.hash);
        db.set_previous_not_empty_block(block.hash, previous_not_empty);
        db.set_block_outcome(block.hash, transitions.clone());
        db.set_block_header(block.hash, block.header.clone());
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
}

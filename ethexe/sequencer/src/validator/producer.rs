use anyhow::{anyhow, Result};
use derivative::Derivative;
use ethexe_common::{
    db::{BlockMetaStorage, CodesStorage, OnChainStorage},
    gear::{BatchCommitment, BlockCommitment, CodeCommitment},
    ProducerBlock, SimpleBlockData,
};
use ethexe_service_utils::Timer;
use ethexe_signer::Address;
use futures::FutureExt;
use gprimitives::H256;
use std::task::Context;

use super::{coordinator::Coordinator, initial::Initial, ValidatorContext, ValidatorSubService};
use crate::ControlEvent;

pub struct Producer {
    ctx: ValidatorContext,
    block: SimpleBlockData,
    validators: Vec<Address>,
    state: State,
}

#[derive(Derivative)]
#[derivative(Debug)]
enum State {
    CollectOffChainTransactions {
        #[derivative(Debug = "ignore")]
        timer: Timer,
    },
    WaitingBlockComputed(H256),
}

impl ValidatorSubService for Producer {
    fn log(&self, s: String) -> String {
        format!("PRODUCER in {state:?} - {s}", state = self.state)
    }

    fn to_dyn(self: Box<Self>) -> Box<dyn ValidatorSubService> {
        self
    }

    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self: Box<Self>) -> ValidatorContext {
        self.ctx
    }

    fn process_computed_block(
        mut self: Box<Self>,
        computed_block: H256,
    ) -> Result<Box<dyn ValidatorSubService>> {
        if matches!(&self.state, State::WaitingBlockComputed(hash) if *hash != computed_block) {
            self.warning(format!("unexpected computed block {computed_block}"));

            return Ok(self);
        }

        let batch = match Self::aggregate_commitments_for_block(&self.ctx, computed_block) {
            Err(AggregationError::SomeBlocksInQueueAreNotComputed(block)) => {
                self.warning(format!(
                    "block {block} in queue for block {computed_block} is not computed"
                ));

                return Initial::create(self.ctx);
            }
            Err(AggregationError::Any(err)) => return Err(err),
            Ok(Some(batch)) => batch,
            Ok(None) => return Initial::create(self.ctx),
        };

        Coordinator::create(self.ctx, self.validators, batch)
    }

    fn poll(mut self: Box<Self>, cx: &mut Context<'_>) -> Result<Box<dyn ValidatorSubService>> {
        match &mut self.state {
            State::CollectOffChainTransactions { timer } => {
                if timer.poll_unpin(cx).is_ready() {
                    self.create_producer_block()?
                }
            }
            State::WaitingBlockComputed(_) => {}
        }

        Ok(self)
    }
}

impl Producer {
    pub fn create(
        mut ctx: ValidatorContext,
        block: SimpleBlockData,
        validators: Vec<Address>,
    ) -> Result<Box<dyn ValidatorSubService>> {
        let mut timer = Timer::new("collect off-chain transactions", ctx.slot_duration / 6);
        timer.start(());

        ctx.pending_events.clear();

        Ok(Box::new(Self {
            ctx,
            block,
            validators,
            state: State::CollectOffChainTransactions { timer },
        }))
    }

    fn aggregate_commitments_for_block(
        ctx: &ValidatorContext,
        block_hash: H256,
    ) -> Result<Option<BatchCommitment>, AggregationError> {
        let block_commitments = Self::aggregate_block_commitments_for_block(ctx, block_hash)?;
        let code_commitments = Self::aggregate_code_commitments_for_block(ctx, block_hash)?;

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
        ctx: &ValidatorContext,
        block_hash: H256,
    ) -> Result<Vec<BlockCommitment>, AggregationError> {
        let commitments_queue = ctx
            .db
            .block_commitment_queue(block_hash)
            .ok_or_else(|| anyhow!("Block {block_hash} commitment queue is not in storage"))?;

        let mut commitments = Vec::new();

        let predecessor_block = block_hash;

        for block in commitments_queue {
            if !ctx.db.block_computed(block) {
                // This can happen when validator syncs from p2p network and skips some old blocks.
                return Err(AggregationError::SomeBlocksInQueueAreNotComputed(block));
            }

            let outcomes = ctx
                .db
                .block_outcome(block)
                .ok_or_else(|| anyhow!("Cannot get from db outcome for computed block {block}"))?;

            let previous_committed_block =
                ctx.db.previous_not_empty_block(block).ok_or_else(|| {
                    anyhow!(
                        "Cannot get from db previous committed block for computed block {block}"
                    )
                })?;

            let header = ctx
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
        ctx: &ValidatorContext,
        block_hash: H256,
    ) -> Result<Vec<CodeCommitment>, AggregationError> {
        Ok(ctx
            .db
            .block_codes_queue(block_hash)
            .ok_or_else(|| anyhow!("Cannot get from db codes queue for block {block_hash}"))?
            .into_iter()
            .filter_map(|code_id| {
                let Some(code_info) = ctx.db.code_blob_info(code_id) else {
                    // TODO +_+_+: this must be an error
                    return None;
                };
                ctx.db.code_valid(code_id).map(|valid| CodeCommitment {
                    id: code_id,
                    timestamp: code_info.timestamp,
                    valid,
                })
            })
            .collect())
    }

    fn create_producer_block(&mut self) -> Result<()> {
        let pb = ProducerBlock {
            block_hash: self.block.hash,
            // TODO +_+_+: set gas allowance here
            gas_allowance: Some(3_000_000_000_000),
            // TODO +_+_+: append off-chain transactions
            off_chain_transactions: Vec::new(),
        };

        let signed_pb = self
            .ctx
            .signer
            .create_signed_data(self.ctx.pub_key, pb.clone())?;

        self.state = State::WaitingBlockComputed(self.block.hash);
        self
            .output(ControlEvent::PublishProducerBlock(signed_pb));
        self.output(ControlEvent::ComputeProducerBlock(pb));

        Ok(())
    }
}

#[derive(Debug, derive_more::From)]
enum AggregationError {
    SomeBlocksInQueueAreNotComputed(H256),
    #[from]
    Any(anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use ethexe_db::{CodeInfo, Database};
    use std::vec;

    // #[tokio::test]
    // async fn producer_new() {
    //     let (signer, _, pub_keys) = init_signer_with_keys(1);
    //     let pub_key = pub_keys[0];
    //     let db = Database::memory();
    //     let validators = vec![Address([1; 20]), Address([2; 20])];
    //     let block = mock_simple_block_data();
    //     let slot_duration = Duration::ZERO;

    //     let producer = Producer::new(
    //         pub_key,
    //         signer.clone(),
    //         db.clone(),
    //         slot_duration,
    //         validators.clone(),
    //         block.clone(),
    //     );
    //     assert_eq!(producer.pub_key, pub_key);
    //     assert_eq!(producer.validators, validators);
    //     assert_eq!(producer.block, block);
    //     assert!(matches!(
    //         producer.state,
    //         State::CollectOffChainTransactions(_)
    //     ));

    //     let events = producer.await.unwrap();
    //     assert_eq!(events.len(), 2);
    //     assert!(matches!(events[0], ControlEvent::PublishProducerBlock(_)));
    //     assert!(matches!(events[1], ControlEvent::ComputeProducerBlock(_)));
    // }

    // #[tokio::test]
    // async fn receive_wrong_computed_block() {
    //     let (signer, _, pub_keys) = init_signer_with_keys(1);
    //     let pub_key = pub_keys[0];
    //     let db = Database::memory();
    //     let validators = vec![Address([1; 20])];
    //     let block = mock_simple_block_data();
    //     let slot_duration = Duration::ZERO;

    //     let mut producer = Producer::new(
    //         pub_key,
    //         signer,
    //         db.clone(),
    //         slot_duration,
    //         validators,
    //         block.clone(),
    //     );
    //     (&mut producer).await.unwrap();

    //     let wrong_block = H256::random();
    //     let events = producer.receive_computed_block(wrong_block).unwrap();
    //     assert!(events.len() == 1 && matches!(events[0], ControlEvent::Warning(_)));
    // }

    // #[tokio::test]
    // async fn code_commitments_only() {
    //     let (signer, _, pub_keys) = init_signer_with_keys(1);
    //     let pub_key = pub_keys[0];
    //     let db = Database::memory();
    //     let validators = vec![Address([1; 20])];
    //     let block = mock_simple_block_data();
    //     let slot_duration = Duration::ZERO;

    //     let mut producer = Producer::new(
    //         pub_key,
    //         signer,
    //         db.clone(),
    //         slot_duration,
    //         validators,
    //         block.clone(),
    //     );

    //     (&mut producer).await.unwrap();

    //     let code1 = prepare_mock_code_commitment(&db);
    //     let code2 = prepare_mock_code_commitment(&db);
    //     db.set_block_codes_queue(block.hash, [code1.id, code2.id].into_iter().collect());
    //     db.set_block_commitment_queue(block.hash, Default::default());

    //     let events = producer.receive_computed_block(block.hash).unwrap();
    //     assert!(events.is_empty());

    //     let (_, _, batch) = producer.into_parts();
    //     let batch = batch.unwrap();
    //     assert_eq!(batch.block_commitments, vec![]);
    //     assert_eq!(batch.code_commitments, vec![code1, code2]);
    // }

    // #[tokio::test]
    // async fn code_and_block_commitments() {
    //     let (signer, _, pub_keys) = init_signer_with_keys(1);
    //     let pub_key = pub_keys[0];
    //     let db = Database::memory();
    //     let validators = vec![Address([1; 20])];
    //     let slot_duration = Duration::ZERO;

    //     let (block1_hash, block2_hash) = (H256::random(), H256::random());
    //     let (block1, block1_commitment) =
    //         prepare_mock_block_commitment(&db, block1_hash, block1_hash, block2_hash);
    //     let (block2, block2_commitment) =
    //         prepare_mock_block_commitment(&db, block2_hash, block1_hash, H256::random());

    //     let mut producer = Producer::new(
    //         pub_key,
    //         signer,
    //         db.clone(),
    //         slot_duration,
    //         validators,
    //         block1.clone(),
    //     );
    //     (&mut producer).await.unwrap();

    //     let code1 = prepare_mock_code_commitment(&db);
    //     let code2 = prepare_mock_code_commitment(&db);

    //     db.set_block_codes_queue(block1.hash, [code1.id, code2.id].into_iter().collect());
    //     db.set_block_commitment_queue(
    //         block1.hash,
    //         [block2.hash, block1.hash].into_iter().collect(),
    //     );

    //     let events = producer.receive_computed_block(block1.hash).unwrap();
    //     assert!(events.is_empty());

    //     let (_, _, batch) = producer.into_parts();
    //     let batch = batch.unwrap();
    //     assert_eq!(
    //         batch.block_commitments,
    //         vec![block2_commitment, block1_commitment]
    //     );
    //     assert_eq!(batch.code_commitments, vec![code1, code2]);
    // }

    // #[test]
    // fn blocks_in_queue_not_computed() {
    //     let (signer, _, pub_keys) = init_signer_with_keys(1);
    //     let pub_key = pub_keys[0];
    //     let db = Database::memory();
    //     let validators = vec![Address([1; 20])];
    //     let block = mock_simple_block_data();

    //     let (mut producer, _) =
    //         Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

    //     let (block1_hash, block2_hash) = (H256::random(), H256::random());
    //     let (block1, _) = prepare_mock_block_commitment(&db, block1_hash, block1_hash, block2_hash);

    //     // Simulate a block in the queue that is not computed
    //     db.set_block_commitment_queue(block.hash, [block1.hash, block2_hash].into_iter().collect());
    //     db.set_block_computed(block1.hash); // Only block1 is marked as computed

    //     let result = producer.receive_computed_block(block.hash);

    //     assert!(matches!(result, Ok(None)));
    // }

    // #[test]
    // fn receive_computed_block_in_collect_off_chain_transactions_state() {
    //     let (signer, _, pub_keys) = init_signer_with_keys(1);
    //     let pub_key = pub_keys[0];
    //     let db = Database::memory();
    //     let validators = vec![Address([1; 20])];
    //     let block = mock_simple_block_data();

    //     let (mut producer, _) =
    //         Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

    //     // Manually set the state to `CollectOffChainTransactions`
    //     producer.state =
    //         ProducerState::CollectOffChainTransactions(Timer::new_from_secs("dead", 10));

    //     let computed_block = block.hash;
    //     let result = producer.receive_computed_block(computed_block);

    //     assert!(matches!(result, Err(ControlError::Fatal(_))));
    // }

    // #[test]
    // fn receive_computed_block_with_wrong_hash() {
    //     let (signer, _, pub_keys) = init_signer_with_keys(1);
    //     let pub_key = pub_keys[0];
    //     let db = Database::memory();
    //     let validators = vec![Address([1; 20])];
    //     let block = mock_simple_block_data();

    //     let (mut producer, _) =
    //         Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

    //     let wrong_block_hash = H256::random();
    //     let result = producer.receive_computed_block(wrong_block_hash);

    //     assert!(matches!(result, Err(ControlError::Warning(_))));
    // }

    // #[test]
    // fn receive_computed_block_in_final_state() {
    //     let (signer, _, pub_keys) = init_signer_with_keys(1);
    //     let pub_key = pub_keys[0];
    //     let db = Database::memory();
    //     let validators = vec![Address([1; 20])];
    //     let block = mock_simple_block_data();

    //     let (mut producer, _) =
    //         Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

    //     // Simulate the producer reaching the final state
    //     producer.state = ProducerState::Final;

    //     let computed_block = block.hash;
    //     let result = producer.receive_computed_block(computed_block);

    //     assert!(matches!(result, Err(ControlError::Fatal(_))));
    // }

    // #[test]
    // fn receive_computed_block_with_missing_commitment_queue() {
    //     let (signer, _, pub_keys) = init_signer_with_keys(1);
    //     let pub_key = pub_keys[0];
    //     let db = Database::memory();
    //     let validators = vec![Address([1; 20])];
    //     let block = mock_simple_block_data();

    //     let (mut producer, _) =
    //         Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

    //     // Simulate missing commitment queue in the database
    //     let computed_block = block.hash;
    //     let result = producer.receive_computed_block(computed_block);

    //     assert!(matches!(result, Err(ControlError::Fatal(_))));
    // }

    // #[test]
    // fn receive_computed_block_with_missing_outcome() {
    //     let (signer, _, pub_keys) = init_signer_with_keys(1);
    //     let pub_key = pub_keys[0];
    //     let db = Database::memory();
    //     let validators = vec![Address([1; 20])];
    //     let block = mock_simple_block_data();

    //     let (mut producer, _) =
    //         Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

    //     // Simulate a block in the queue but missing outcome
    //     let block1_hash = H256::random();
    //     db.set_block_commitment_queue(block.hash, [block1_hash].into_iter().collect());
    //     db.set_block_computed(block1_hash);

    //     let computed_block = block.hash;
    //     let result = producer.receive_computed_block(computed_block);

    //     assert!(matches!(result, Err(ControlError::Fatal(_))));
    // }

    // #[test]
    // fn receive_computed_block_with_missing_previous_committed_block() {
    //     let (signer, _, pub_keys) = init_signer_with_keys(1);
    //     let pub_key = pub_keys[0];
    //     let db = Database::memory();
    //     let validators = vec![Address([1; 20])];
    //     let block = mock_simple_block_data();

    //     let (mut producer, _) =
    //         Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

    //     // Simulate a block in the queue but missing previous committed block
    //     let block1_hash = H256::random();
    //     db.set_block_commitment_queue(block.hash, [block1_hash].into_iter().collect());
    //     db.set_block_computed(block1_hash);
    //     db.set_block_outcome(block1_hash, vec![mock_state_transition()]);

    //     let computed_block = block.hash;
    //     let result = producer.receive_computed_block(computed_block);

    //     assert!(matches!(result, Err(ControlError::Fatal(_))));
    // }

    // #[test]
    // fn receive_computed_block_with_missing_header() {
    //     let (signer, _, pub_keys) = init_signer_with_keys(1);
    //     let pub_key = pub_keys[0];
    //     let db = Database::memory();
    //     let validators = vec![Address([1; 20])];
    //     let block = mock_simple_block_data();

    //     let (mut producer, _) =
    //         Producer::new(pub_key, signer, db.clone(), validators, block.clone()).unwrap();

    //     // Simulate a block in the queue but missing header
    //     let block1_hash = H256::random();
    //     db.set_block_commitment_queue(block.hash, [block1_hash].into_iter().collect());
    //     db.set_block_computed(block1_hash);
    //     db.set_block_outcome(block1_hash, vec![mock_state_transition()]);
    //     db.set_previous_not_empty_block(block1_hash, H256::random());

    //     let computed_block = block.hash;
    //     let result = producer.receive_computed_block(computed_block);

    //     assert!(matches!(result, Err(ControlError::Fatal(_))));
    // }

    // #[test]
    // fn into_parts_works() {
    //     let (signer, _, pub_keys) = init_signer_with_keys(1);
    //     let pub_key = pub_keys[0];
    //     let db = Database::memory();
    //     let validators = vec![Address([1; 20]), Address([2; 20])];
    //     let block = mock_simple_block_data();

    //     let (mut producer, _) = Producer::new(
    //         pub_key,
    //         signer,
    //         db.clone(),
    //         validators.clone(),
    //         block.clone(),
    //     )
    //     .unwrap();

    //     // Simulate the producer reaching the final state
    //     producer.state = ProducerState::Final;

    //     let (returned_validators, returned_block) = producer.into_parts();

    //     assert_eq!(returned_validators, validators);
    //     assert_eq!(returned_block, block);
    // }

    // #[test]
    // #[should_panic(expected = "Producer is not in the final state: wrong Producer usage")]
    // fn into_parts_panics_if_not_final() {
    //     let (signer, _, pub_keys) = init_signer_with_keys(1);
    //     let pub_key = pub_keys[0];
    //     let db = Database::memory();
    //     let validators = vec![Address([1; 20]), Address([2; 20])];
    //     let block = mock_simple_block_data();

    //     let (producer, _) = Producer::new(pub_key, signer, db.clone(), validators, block).unwrap();

    //     // Attempt to call into_parts without reaching the final state
    //     let _ = producer.into_parts();
    // }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

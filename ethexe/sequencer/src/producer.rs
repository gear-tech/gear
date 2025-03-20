use crate::{
    agro::SignedCommitmentsBatch,
    bp::{ControlError, ControlEvent, ProducerBlock, SignedProducerBlock},
};
use anyhow::anyhow;
use ethexe_common::{
    db::{BlockMetaStorage, CodesStorage, OnChainStorage},
    gear::{BlockCommitment, CodeCommitment},
    SimpleBlockData,
};
use ethexe_db::Database;
use ethexe_signer::{Address, PublicKey, Signer, ToDigest};
use gprimitives::H256;

pub struct Producer {
    pub_key: PublicKey,
    signer: Signer,
    db: Database,
    validators: Vec<Address>,
    block: SimpleBlockData,
    state: ProducerState,
}

pub enum ProducerState {
    #[allow(unused)]
    CollectOffChainTransactions,
    WaitingBlockComputed(H256),
}

impl Producer {
    pub fn new(
        pub_key: PublicKey,
        signer: Signer,
        db: Database,
        validators: Vec<Address>,
        block: SimpleBlockData,
    ) -> Result<(Self, Vec<ControlEvent>), ControlError> {
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
            // +_+_+ set gas allowance here
            gas_allowance: Some(3_000_000_000_000),
            // +_+_+ append off-chain transactions
            off_chain_transactions: Vec::new(),
        };

        let ecdsa_signature = producer
            .signer
            .sign_digest(producer.pub_key, block.to_digest())?;

        Ok((
            producer,
            vec![
                ControlEvent::ComputeProducerBlock(block.clone()),
                ControlEvent::PublishProducerBlock(SignedProducerBlock {
                    block,
                    ecdsa_signature,
                }),
            ],
        ))
    }

    pub fn receive_computed_block(
        &mut self,
        computed_block: H256,
    ) -> Result<Option<SignedCommitmentsBatch>, ControlError> {
        match &mut self.state {
            ProducerState::CollectOffChainTransactions => Err(ControlError::Common(anyhow!(
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

                self.aggregate_commitments_for_block(computed_block)
            }
        }
    }

    // TODO (gsobol): make test for this method
    fn aggregate_commitments_for_block(
        &self,
        block_hash: H256,
    ) -> Result<Option<SignedCommitmentsBatch>, ControlError> {
        let block_commitments = self.aggregate_block_commitments_for_block(block_hash)?;
        let code_commitments = self.aggregate_code_commitments_for_block(block_hash)?;
        SignedCommitmentsBatch::new(
            block_commitments,
            code_commitments,
            &self.signer,
            self.pub_key,
        )
        .map_err(Into::into)
    }

    fn aggregate_block_commitments_for_block(
        &self,
        block_hash: H256,
    ) -> Result<Vec<BlockCommitment>, ControlError> {
        let commitments_queue = self
            .db
            .block_commitment_queue(block_hash)
            .ok_or_else(|| anyhow!("Block {block_hash} is not in storage"))?;

        if commitments_queue.is_empty() {
            // Nothing to commit
            return Ok(vec![]);
        }

        let mut commitments = Vec::new();

        let predecessor_block = block_hash;

        for block in commitments_queue {
            if !self.db.block_computed(block) {
                // This can happen when validator syncs from p2p network and skips some old blocks.
                return Err(ControlError::Warning(anyhow!(
                    "Block in commitment queue {block} is not computed"
                )));
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
    ) -> Result<Vec<CodeCommitment>, ControlError> {
        Ok(self
            .db
            .block_codes_queue(block_hash)
            .ok_or_else(|| anyhow!("Cannot get from db codes queue for block {block_hash}"))?
            .into_iter()
            .filter_map(|code_id| {
                let Some(code_info) = self.db.code_blob_info(code_id) else {
                    // +_+_+ fix this
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

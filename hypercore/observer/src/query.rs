use std::{
    collections::{BTreeSet, VecDeque},
    sync::Arc,
};

use crate::{
    observer::{read_block_events, read_code_from_tx_hash, ObserverProvider},
    BlobReader,
};
use alloy::{
    primitives::Address,
    providers::{Provider, ProviderBuilder},
    rpc::types::eth::BlockTransactionsKind,
};
use anyhow::{anyhow, Result};
use gprimitives::{ActorId, CodeId, H256};
use hypercore_common::{
    db::{BlockHeader, BlockMetaStorage},
    events::BlockEvent,
};
use hypercore_signer::Address as HypercoreAddress;

pub struct Query {
    database: Box<dyn BlockMetaStorage>,
    provider: ObserverProvider,
    router_address: Address,
    genesis_block_hash: H256,
    blob_reader: Arc<dyn BlobReader>,
    max_commitment_depth: u32,
}

impl Query {
    pub async fn new(
        database: Box<dyn BlockMetaStorage>,
        ethereum_rpc: &str,
        router_address: HypercoreAddress,
        genesis_block_hash: H256,
        blob_reader: Arc<dyn BlobReader>,
        max_commitment_depth: u32,
    ) -> Result<Self> {
        // Init db for genesis block
        database.set_block_commitment_queue(genesis_block_hash, Default::default());
        database.set_block_prev_commitment(genesis_block_hash, H256::zero());
        database.set_block_end_state_is_valid(genesis_block_hash, true);
        database.set_block_is_empty(genesis_block_hash, true);
        database.set_block_end_program_states(genesis_block_hash, Default::default());

        Ok(Self {
            database,
            provider: ProviderBuilder::new().on_builtin(ethereum_rpc).await?,
            router_address: Address::new(router_address.0),
            genesis_block_hash,
            blob_reader,
            max_commitment_depth,
        })
    }

    async fn get_committed_blocks(&mut self, block_hash: H256) -> Result<BTreeSet<H256>> {
        Ok(self
            .get_block_events(block_hash)
            .await?
            .into_iter()
            .filter_map(|event| {
                if let BlockEvent::BlockCommitted(event) = event {
                    Some(event.block_hash)
                } else {
                    None
                }
            })
            .collect())
    }

    pub async fn get_last_committed_chain(&mut self, block_hash: H256) -> Result<Vec<H256>> {
        let mut chain = Vec::new();
        let mut committed_blocks = BTreeSet::new();

        // First we need to find the nearest valid block
        let mut hash = block_hash;
        loop {
            // TODO: limit deepness

            if hash == self.genesis_block_hash {
                // Genesis block is always valid
                log::trace!("Genesis block reached: {hash}");
                break;
            }

            if self
                .database
                .block_end_state_is_valid(hash)
                .unwrap_or(false)
            {
                log::trace!("Nearest valid in db block found: {hash}");
                break;
            }

            log::trace!("Include block {hash} in chain for processing");
            chain.push(hash);

            committed_blocks.extend(self.get_committed_blocks(hash).await?);

            hash = self.get_block_parent_hash(hash).await?;
        }

        let mut actual_commitment_queue: VecDeque<H256> = self
            .database
            .block_commitment_queue(hash)
            .ok_or(anyhow!("commitment queue not found for block {hash}"))?
            .into_iter()
            .filter(|hash| !committed_blocks.contains(hash))
            .collect();

        let Some(oldest_not_committed_block) = actual_commitment_queue.pop_front() else {
            // All blocks before nearest valid block are committed,
            // so we need to execute all blocks from valid to current.
            return Ok(chain);
        };

        // Now we need append in chain all blocks from the oldest not committed to the current.
        let mut depth = 0;
        loop {
            if depth >= self.max_commitment_depth {
                return Err(anyhow!("too deep chain"));
            }

            log::trace!("Include block {hash} in chain for processing");
            chain.push(hash);

            if hash == oldest_not_committed_block {
                log::trace!("Oldest not committed block reached: {hash}");
                break;
            }

            hash = self.get_block_parent_hash(hash).await?;
            depth += 1;
        }

        Ok(chain)
    }

    pub async fn propagate_meta_for_block(&mut self, block_hash: H256) -> Result<()> {
        let parent = self.get_block_parent_hash(block_hash).await?;

        if !self
            .database
            .block_end_state_is_valid(parent)
            .unwrap_or(false)
        {
            return Err(anyhow!("parent block is not valid"));
        }

        // Propagate program state hashes
        let program_state_hashes = self
            .database
            .block_end_program_states(parent)
            .ok_or(anyhow!("parent block end states not found"))?;
        self.database
            .set_block_start_program_states(block_hash, program_state_hashes);

        // Propagate `wait for commitment` blocks queue
        let queue = self
            .database
            .block_commitment_queue(parent)
            .ok_or(anyhow!("parent block commitment queue not found"))?;
        let committed_blocks = self.get_committed_blocks(block_hash).await?;
        let current_queue = queue
            .into_iter()
            .filter(|hash| !committed_blocks.contains(hash))
            .collect();
        self.database
            .set_block_commitment_queue(block_hash, current_queue);

        // Propagate prev commitment (prev not empty block hash or zero for genesis).
        if self
            .database
            .block_is_empty(parent)
            .ok_or(anyhow!("Cannot identify whether parent is empty"))?
        {
            let parent_prev_commitment = self
                .database
                .block_prev_commitment(parent)
                .ok_or(anyhow!("parent block prev commitment not found"))?;
            self.database
                .set_block_prev_commitment(block_hash, parent_prev_commitment);
        } else {
            self.database.set_block_prev_commitment(block_hash, parent);
        }

        Ok(())
    }

    pub async fn get_block_header_meta(&mut self, block_hash: H256) -> Result<BlockHeader> {
        match self.database.block_header(block_hash) {
            Some(meta) => Ok(meta),
            None => {
                let block = self
                    .provider
                    .get_block_by_hash(block_hash.0.into(), BlockTransactionsKind::Hashes)
                    .await?
                    .ok_or(anyhow!("block not found"))?;

                let height = u32::try_from(
                    block
                        .header
                        .number
                        .ok_or(anyhow!("block number not found"))?,
                )
                .unwrap_or_else(|err| unreachable!("Ethereum block number not fit in u32: {err}"));
                let timestamp = block.header.timestamp;
                let parent_hash = H256(block.header.parent_hash.0);

                let meta = BlockHeader {
                    height,
                    timestamp,
                    parent_hash,
                };

                self.database.set_block_header(block_hash, meta.clone());

                Ok(meta)
            }
        }
    }

    pub async fn get_block_parent_hash(&mut self, block_hash: H256) -> Result<H256> {
        Ok(self.get_block_header_meta(block_hash).await?.parent_hash)
    }

    pub async fn get_block_events(&mut self, block_hash: H256) -> Result<Vec<BlockEvent>> {
        if let Some(events) = self.database.block_events(block_hash) {
            return Ok(events);
        }

        let events = read_block_events(block_hash, &self.provider, self.router_address).await?;
        self.database.set_block_events(block_hash, events.clone());

        Ok(events)
    }

    pub async fn download_code(
        &self,
        code_id: CodeId,
        origin: ActorId,
        tx_hash: H256,
    ) -> Result<Vec<u8>> {
        let blob_reader = self.blob_reader.clone();
        let attempts = Some(3);

        read_code_from_tx_hash(blob_reader, origin, tx_hash, attempts, code_id)
            .await
            .map(|res| res.2)
    }
}

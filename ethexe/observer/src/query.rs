use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    sync::Arc,
};

use crate::{
    observer::{
        read_block_events, read_block_request_events, read_block_request_events_batch,
        read_code_from_tx_hash, ObserverProvider,
    },
    BlobReader,
};
use alloy::{
    eips::BlockNumberOrTag,
    primitives::Address as AlloyAddress,
    providers::{Provider, ProviderBuilder},
    rpc::types::eth::BlockTransactionsKind,
};
use anyhow::{anyhow, Result};
use ethexe_common::{
    db::{BlockHeader, BlockMetaStorage},
    events::{BlockEvent, BlockRequestEvent, RouterEvent},
};
use ethexe_signer::Address;
use gprimitives::{CodeId, H256};

/// Height difference to start fast sync.
const DEEP_SYNC: u32 = 10;

#[derive(Clone)]
pub struct Query {
    database: Arc<dyn BlockMetaStorage>,
    provider: ObserverProvider,
    router_address: AlloyAddress,
    genesis_block_hash: H256,
    blob_reader: Arc<dyn BlobReader>,
    max_commitment_depth: u32,
}

impl Query {
    pub async fn new(
        database: Arc<dyn BlockMetaStorage>,
        ethereum_rpc: &str,
        router_address: Address,
        genesis_block_hash: H256,
        blob_reader: Arc<dyn BlobReader>,
        max_commitment_depth: u32,
    ) -> Result<Self> {
        let mut query = Self {
            database,
            provider: ProviderBuilder::new().on_builtin(ethereum_rpc).await?,
            router_address: AlloyAddress::new(router_address.0),
            genesis_block_hash,
            blob_reader,
            max_commitment_depth,
        };
        // Initialize the database for the genesis block
        query.init_genesis_block().await?;

        Ok(query)
    }

    async fn init_genesis_block(&mut self) -> Result<()> {
        let hash = self.genesis_block_hash;
        self.database
            .set_block_commitment_queue(hash, Default::default());
        self.database.set_block_prev_commitment(hash, H256::zero());
        self.database.set_block_end_state_is_valid(hash, true);
        self.database.set_block_is_empty(hash, true);
        self.database
            .set_block_end_program_states(hash, Default::default());
        self.database
            .set_block_end_schedule(hash, Default::default());

        // set latest valid if empty.
        if self.database.latest_valid_block().is_none() {
            let genesis_header = self.get_block_header_meta(hash).await?;
            self.database.set_latest_valid_block(hash, genesis_header);
        }

        Ok(())
    }

    async fn get_committed_blocks(&mut self, block_hash: H256) -> Result<BTreeSet<H256>> {
        // TODO (breathx): optimize me ASAP.
        Ok(self
            .get_block_events(block_hash)
            .await?
            .into_iter()
            .filter_map(|event| match event {
                BlockEvent::Router(RouterEvent::BlockCommitted { hash }) => Some(hash),
                _ => None,
            })
            .collect())
    }

    /// Populate database with blocks using rpc provider.
    async fn load_chain_batch(
        &mut self,
        from_block: u32,
        to_block: u32,
    ) -> Result<HashMap<H256, BlockHeader>> {
        let total_blocks = to_block.saturating_sub(from_block) + 1;
        log::info!("Starting to load {total_blocks} blocks from {from_block} to {to_block}");

        let fetches = (from_block..=to_block).map(|block_number| {
            let provider = self.provider.clone();
            let database = Arc::clone(&self.database);
            tokio::spawn(async move {
                let block = provider
                    .get_block_by_number(BlockNumberOrTag::Number(block_number as u64), false)
                    .await?;
                let block = block
                    .ok_or_else(|| anyhow!("Block not found for block number {block_number}"))?;

                let height = u32::try_from(block.header.number)
                    .map_err(|err| anyhow!("Ethereum block number not fit in u32: {err}"))?;
                let timestamp = block.header.timestamp;
                let block_hash = H256(block.header.hash.0);
                let parent_hash = H256(block.header.parent_hash.0);

                let header = BlockHeader {
                    height,
                    timestamp,
                    parent_hash,
                };

                database.set_block_header(block_hash, header.clone());

                Ok::<(H256, BlockHeader), anyhow::Error>((block_hash, header))
            })
        });

        // Fetch events in block range.
        let mut blocks_events = read_block_request_events_batch(
            from_block,
            to_block,
            &self.provider,
            self.router_address,
        )
        .await?;

        // Collect results
        let mut block_headers = HashMap::new();
        for result in futures::future::join_all(fetches).await {
            let (block_hash, header) = result??;
            // Set block events, empty vec if no events.
            self.database.set_block_events(
                block_hash,
                blocks_events.remove(&block_hash).unwrap_or_default(),
            );
            block_headers.insert(block_hash, header);
        }
        log::trace!("{} blocks loaded", block_headers.len());

        Ok(block_headers)
    }

    pub async fn get_last_committed_chain(&mut self, block_hash: H256) -> Result<Vec<H256>> {
        let current_block = self.get_block_header_meta(block_hash).await?;
        let latest_valid_block_height = self
            .database
            .latest_valid_block()
            .map(|(_, header)| header.height)
            .expect("genesis by default; qed");

        if current_block.height >= latest_valid_block_height
            && (current_block.height - latest_valid_block_height) >= self.max_commitment_depth
        {
            return Err(anyhow!(
                "Too deep chain: Current block height: {}, Latest valid block height: {}, Max depth: {}",
                current_block.height,
                latest_valid_block_height,
                self.max_commitment_depth
            ));
        }

        // Determine if deep sync is needed
        let is_deep_sync = {
            // Current block can be lower than latest valid due to reorgs.
            let block_diff = current_block
                .height
                .saturating_sub(latest_valid_block_height);
            block_diff > DEEP_SYNC
        };

        let mut chain = Vec::new();
        let mut committed_blocks = Vec::new();
        let mut headers_map = HashMap::new();

        if is_deep_sync {
            // Load blocks in batch from provider by numbers.
            headers_map = self
                .load_chain_batch(latest_valid_block_height + 1, current_block.height)
                .await?;
        }

        // Continue loading chain by parent hashes from the current block to the latest valid block.
        let mut hash = block_hash;

        while hash != self.genesis_block_hash {
            // If the block's end state is valid, set it as the latest valid block
            if self
                .database
                .block_end_state_is_valid(hash)
                .unwrap_or(false)
            {
                let header = match headers_map.get(&hash) {
                    Some(header) => header.clone(),
                    None => self.get_block_header_meta(hash).await?,
                };

                self.database.set_latest_valid_block(hash, header);

                log::trace!("Nearest valid in db block found: {hash}");
                break;
            }

            log::trace!("Include block {hash} in chain for processing");
            committed_blocks.extend(self.get_committed_blocks(hash).await?);
            chain.push(hash);

            // Fetch parent hash from headers_map or database
            hash = match headers_map.get(&hash) {
                Some(header) => header.parent_hash,
                None => self.get_block_parent_hash(hash).await?,
            };
        }

        let mut actual_commitment_queue: VecDeque<H256> = self
            .database
            .block_commitment_queue(hash)
            .ok_or_else(|| {
                anyhow!(
                    "Commitment queue not found for block {hash}, possible database inconsistency."
                )
            })?
            .into_iter()
            .filter(|hash| !committed_blocks.contains(hash))
            .collect();

        let Some(oldest_not_committed_block) = actual_commitment_queue.pop_front() else {
            // All blocks before nearest valid block are committed,
            // so we need to execute all blocks from valid to current.
            return Ok(chain);
        };

        while hash != oldest_not_committed_block {
            log::trace!("Include block {hash} in chain for processing");
            chain.push(hash);

            hash = self.get_block_parent_hash(hash).await?;
        }

        log::trace!("Oldest not committed block reached: {}", hash);
        chain.push(hash);
        Ok(chain)
    }

    pub async fn propagate_meta_for_block(&mut self, block_hash: H256) -> Result<()> {
        let parent = self.get_block_parent_hash(block_hash).await?;

        if !self
            .database
            .block_end_state_is_valid(parent)
            .unwrap_or(false)
        {
            return Err(anyhow!("parent block is not valid for block {block_hash}"));
        }

        // Propagate program state hashes
        let program_state_hashes = self
            .database
            .block_end_program_states(parent)
            .ok_or_else(|| anyhow!("parent block end states not found"))?;
        self.database
            .set_block_start_program_states(block_hash, program_state_hashes);

        // Propagate scheduled tasks
        let schedule = self
            .database
            .block_end_schedule(parent)
            .ok_or_else(|| anyhow!("parent block schedule not found"))?;
        self.database.set_block_start_schedule(block_hash, schedule);

        // Propagate `wait for commitment` blocks queue
        let queue = self
            .database
            .block_commitment_queue(parent)
            .ok_or_else(|| anyhow!("parent block commitment queue not found"))?;
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
            .ok_or_else(|| anyhow!("Cannot identify whether parent is empty"))?
        {
            let parent_prev_commitment = self
                .database
                .block_prev_commitment(parent)
                .ok_or_else(|| anyhow!("parent block prev commitment not found"))?;
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
                    .ok_or_else(|| anyhow!("Block not found"))?;

                let height = u32::try_from(block.header.number).unwrap_or_else(|err| {
                    unreachable!("Ethereum block number not fit in u32: {err}")
                });
                let timestamp = block.header.timestamp;
                let parent_hash = H256(block.header.parent_hash.0);

                let meta = BlockHeader {
                    height,
                    timestamp,
                    parent_hash,
                };

                self.database.set_block_header(block_hash, meta.clone());

                // Populate block events in db.
                let events =
                    read_block_request_events(block_hash, &self.provider, self.router_address)
                        .await?;
                self.database.set_block_events(block_hash, events);

                Ok(meta)
            }
        }
    }

    pub async fn get_block_parent_hash(&mut self, block_hash: H256) -> Result<H256> {
        Ok(self.get_block_header_meta(block_hash).await?.parent_hash)
    }

    pub async fn get_block_events(&mut self, block_hash: H256) -> Result<Vec<BlockEvent>> {
        read_block_events(block_hash, &self.provider, self.router_address).await
    }

    pub async fn get_block_request_events(
        &mut self,
        block_hash: H256,
    ) -> Result<Vec<BlockRequestEvent>> {
        if let Some(events) = self.database.block_events(block_hash) {
            return Ok(events);
        }

        let events =
            read_block_request_events(block_hash, &self.provider, self.router_address).await?;
        self.database.set_block_events(block_hash, events.clone());

        Ok(events)
    }

    pub async fn download_code(
        &self,
        expected_code_id: CodeId,
        blob_tx_hash: H256,
    ) -> Result<Vec<u8>> {
        let blob_reader = self.blob_reader.clone();
        let attempts = Some(3);

        read_code_from_tx_hash(blob_reader, expected_code_id, blob_tx_hash, attempts)
            .await
            .map(|res| res.1)
    }
}

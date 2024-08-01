use std::{
    collections::{BTreeSet, VecDeque},
    sync::Arc,
};

use crate::{
    observer::{
        read_block_events, read_block_events_batch, read_code_from_tx_hash, ObserverProvider,
        MAX_QUERY_BLOCK_RANGE,
    },
    BlobReader,
};
use alloy::{
    eips::BlockNumberOrTag,
    primitives::{Address as AlloyAddress, B256},
    providers::{Provider, ProviderBuilder},
    rpc::types::{eth::BlockTransactionsKind, Filter},
};
use anyhow::{anyhow, Result};
use ethexe_common::{
    db::{BlockHeader, BlockMetaStorage},
    events::{BlockCommitted, BlockEvent},
};
use ethexe_ethereum::event::{match_log, signature_hash};
use ethexe_signer::Address;
use gprimitives::{ActorId, CodeId, H256};

/// Height difference to start fast sync.
const DEEP_SYNC: u32 = 100;

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
        // Initialize the database for the genesis block
        Self::init_genesis_block(&database, genesis_block_hash)?;

        Ok(Self {
            database,
            provider: ProviderBuilder::new().on_builtin(ethereum_rpc).await?,
            router_address: AlloyAddress::new(router_address.0),
            genesis_block_hash,
            blob_reader,
            max_commitment_depth,
        })
    }

    fn init_genesis_block(
        database: &Arc<dyn BlockMetaStorage>,
        genesis_block_hash: H256,
    ) -> Result<()> {
        database.set_block_commitment_queue(genesis_block_hash, Default::default());
        database.set_block_prev_commitment(genesis_block_hash, H256::zero());
        database.set_block_end_state_is_valid(genesis_block_hash, true);
        database.set_block_is_empty(genesis_block_hash, true);
        database.set_block_end_program_states(genesis_block_hash, Default::default());
        Ok(())
    }

    async fn get_committed_blocks(&mut self, block_hash: H256) -> Result<BTreeSet<H256>> {
        Ok(self
            .get_block_events(block_hash)
            .await?
            .into_iter()
            .filter_map(|event| match event {
                BlockEvent::BlockCommitted(event) => Some(event.block_hash),
                _ => None,
            })
            .collect())
    }

    async fn get_all_committed_blocks(
        &mut self,
        from_block: u32,
        to_block: u32,
    ) -> Result<Vec<H256>> {
        let mut committed_blocks = vec![];
        let mut start_block = from_block;

        while start_block <= to_block {
            let end_block = std::cmp::min(start_block + MAX_QUERY_BLOCK_RANGE - 1, to_block);

            let router_events_filter = Filter::new()
                .from_block(start_block as u64)
                .to_block(end_block as u64)
                .address(self.router_address)
                .event_signature(B256::new(signature_hash::BLOCK_COMMITTED));

            let logs = self.provider.get_logs(&router_events_filter).await?;

            for log in logs.iter() {
                if let Some(BlockEvent::BlockCommitted(BlockCommitted { block_hash })) =
                    match_log(log)?
                {
                    committed_blocks.push(block_hash);
                }
            }

            log::trace!("Read committed blocks from {from_block} to {to_block}");

            start_block = end_block + 1;
        }

        Ok(committed_blocks)
    }

    async fn get_latest_valid_block(&mut self) -> Result<(H256, BlockHeader)> {
        if let Some(latest_valid_block_hash) = self.database.latest_valid_block() {
            let latest_valid_header =
                self.database
                    .block_header(latest_valid_block_hash)
                    .ok_or(anyhow!(
                        "{latest_valid_block_hash} not found in db. Corrupted"
                    ))?;

            let chain_block = self
                .provider
                .get_block_by_number((latest_valid_header.height as u64).into(), false)
                .await?;

            match chain_block {
                Some(block)
                    if block.header.hash.map(|h| h.0) == Some(latest_valid_block_hash.0) =>
                {
                    Ok((latest_valid_block_hash, latest_valid_header))
                }
                Some(block) => {
                    let finalized_block = self
                        .provider
                        .get_block_by_number(BlockNumberOrTag::Finalized, false)
                        .await?
                        .ok_or(anyhow!("Failed to get finalized block"))?;

                    if finalized_block.header.number.unwrap() >= latest_valid_header.height as u64 {
                        log::warn!("Latest valid block doesn't match on-chain block.");
                        let hash = H256(block.header.hash.unwrap().0);
                        Ok((hash, self.get_block_header_meta(hash).await?))
                    } else {
                        Ok((latest_valid_block_hash, latest_valid_header))
                    }
                }
                None => Ok((latest_valid_block_hash, latest_valid_header)),
            }
        } else {
            log::debug!("Latest valid block not found, sync to genesis.");
            Ok((
                self.genesis_block_hash,
                self.get_block_header_meta(self.genesis_block_hash).await?,
            ))
        }
    }

    /// Populate database with blocks using rpc provider.
    async fn load_chain_batch(
        &self,
        from_block: u32,
        to_block: u32,
    ) -> Result<Vec<(H256, BlockHeader)>> {
        let fetches = (from_block..=to_block).map(|block_number| {
            self.provider
                .get_block_by_number(BlockNumberOrTag::Number(block_number as u64), false)
        });

        let blocks = futures::future::join_all(fetches).await;

        log::trace!("{} blocks loaded", blocks.len());

        // Fetch events in block range.
        let mut blocks_events =
            read_block_events_batch(from_block, to_block, &self.provider, self.router_address)
                .await?;

        // Populate blocks in db.
        let mut headers = Vec::new();
        for block in blocks {
            let block = block?.ok_or(anyhow!("Block not found"))?;
            let height = u32::try_from(
                block
                    .header
                    .number
                    .ok_or(anyhow!("Block number not found"))?,
            )
            .unwrap_or_else(|err| unreachable!("Ethereum block number not fit in u32: {err}"));
            let timestamp = block.header.timestamp;
            let block_hash = H256(block.header.hash.unwrap().0);
            let parent_hash = H256(block.header.parent_hash.0);

            let header = BlockHeader {
                height,
                timestamp,
                parent_hash,
            };

            self.database.set_block_header(block_hash, header.clone());

            // Set block events, empty vec if no events.
            self.database.set_block_events(
                block_hash,
                blocks_events.remove(&block_hash).unwrap_or_default(),
            );

            headers.push((block_hash, header));
        }

        // Sort headers by height from big to small
        headers.sort_by(|a, b| b.1.height.cmp(&a.1.height));

        Ok(headers)
    }

    pub async fn load_chain(
        &mut self,
        from_hash: H256,
        to_hash: H256,
    ) -> Result<(Vec<H256>, Vec<H256>)> {
        let mut chain = vec![];
        let mut committed_blocks = Vec::new();
        let mut hash = from_hash;

        while hash != to_hash {
            // If the block's end state is valid, set it as the latest valid block
            if self
                .database
                .block_end_state_is_valid(hash)
                .unwrap_or(false)
            {
                self.database.set_latest_valid_block(hash);
                log::trace!("Nearest valid in db block found: {hash}");
                break;
            }

            log::trace!("Include block {hash} in chain for processing");
            committed_blocks.extend(self.get_committed_blocks(hash).await?);
            chain.push(hash);

            match self.database.block_header(hash) {
                Some(block_header) => hash = block_header.parent_hash,
                None => {
                    hash = self.get_block_parent_hash(hash).await?;
                    log::trace!("Block was fetched from alloy: {hash}");
                }
            }
        }

        Ok((chain, committed_blocks))
    }

    pub async fn get_last_committed_chain(&mut self, block_hash: H256) -> Result<Vec<H256>> {
        let mut chain = Vec::new();

        // Get the metadata for the current block.
        let current_block = self.get_block_header_meta(block_hash).await?;

        // Find the latest valid block or use genesis.
        let (latest_valid_block_hash, latest_valid_block) = self.get_latest_valid_block().await?;

        // Check for deep chain.
        if current_block.height >= latest_valid_block.height
            && (current_block.height - latest_valid_block.height) >= self.max_commitment_depth
        {
            return Err(anyhow!(
                "Too deep chain: Current block height: {}, Latest valid block height: {}, Max depth: {}",
                current_block.height,
                latest_valid_block.height,
                self.max_commitment_depth
            ));
        }

        let mut committed_blocks = BTreeSet::new();

        // Determine if deep sync is needed
        let is_deep_sync = {
            // Current block can be lower than latest valid due to reorgs.
            let block_diff = (current_block.height as i64 - latest_valid_block.height as i64)
                .unsigned_abs() as u32;
            if block_diff > DEEP_SYNC {
                let chain_block = self
                    .provider
                    .get_block_by_number(
                        (current_block.height.saturating_sub(DEEP_SYNC) as u64).into(),
                        false,
                    )
                    .await?;
                let hash = H256(chain_block.unwrap().header.hash.unwrap().0);

                // Check if the block hash for the deep sync height exists in the database.
                self.database.block_header(hash).is_none()
            } else {
                false
            }
        };

        // Populate db to the latest valid block.
        let mut hash = block_hash;

        if is_deep_sync {
            // Load all blocks from provider by numbers, skip latest valid.
            let headers = self
                .load_chain_batch(latest_valid_block.height + 1, current_block.height)
                .await?;
            for (block_hash, _header) in headers {
                chain.push(block_hash);
                hash = block_hash;
            }

            hash = self.get_block_parent_hash(hash).await?;

            // Collect committed blocks if block height difference is significant.
            committed_blocks.extend(
                self.get_all_committed_blocks(latest_valid_block.height, current_block.height)
                    .await?,
            );
        } else {
            // Load chain by parent hashes.
            let (load, comm_blocks) = self.load_chain(block_hash, latest_valid_block_hash).await?;
            committed_blocks.extend(comm_blocks);

            chain = load;

            hash = *chain.last().expect("qed; can't be empty");

            hash = self.get_block_parent_hash(hash).await?;
        }

        let mut actual_commitment_queue: VecDeque<H256> = self
            .database
            .block_commitment_queue(hash)
            .ok_or(anyhow!(
                "Commitment queue not found for block {hash}, possible database inconsistency."
            ))?
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
                    .ok_or(anyhow!("Block not found"))?;

                let height = u32::try_from(
                    block
                        .header
                        .number
                        .ok_or(anyhow!("Block number not found"))?,
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

                // Populate block events in db.
                let events =
                    read_block_events(block_hash, &self.provider, self.router_address).await?;
                self.database.set_block_events(block_hash, events.clone());

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

        log::info!("read_block_events {block_hash}");
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

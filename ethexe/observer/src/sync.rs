use crate::{BlobReader, Provider, MAX_QUERY_BLOCK_RANGE};
use alloy::{
    network::{Ethereum, Network},
    primitives::Address as AlloyAddress,
    providers::Provider as _,
    rpc::{
        client::BatchRequest,
        types::{eth::Header, Filter},
    },
};
use anyhow::{anyhow, Result};
use ethexe_common::{
    db::BlocksOnChainData,
    events::{BlockEvent, RouterEvent},
    BlockData,
};
use ethexe_db::{BlockHeader, CodeInfo};
use futures::{
    future::{self},
    stream::FuturesUnordered,
    FutureExt,
};
use gprimitives::{CodeId, H256};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub(crate) struct ChainSync {
    pub provider: Provider,
    pub database: Box<dyn BlocksOnChainData>,
    pub blobs_reader: Arc<dyn BlobReader>,
    pub router_address: AlloyAddress,
    pub wvara_address: AlloyAddress,
    pub max_sync_depth: u32,
    pub heuristic_sync_depth: u32,
}

impl ChainSync {
    async fn load_blocks_batch_data(
        provider: Provider,
        router_address: AlloyAddress,
        wvara_address: AlloyAddress,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<BlockData>> {
        log::trace!("Querying blocks from {from_block} to {to_block}");

        let mut batch = BatchRequest::new(provider.client());

        let headers_request: FuturesUnordered<_> = (from_block..=to_block)
            .map(|bn| {
                batch
                    .add_call::<_, Option<<Ethereum as Network>::BlockResponse>>(
                        "eth_getBlockByNumber",
                        &(format!("0x{bn:x}"), false),
                    )
                    .expect("infallible")
                    .boxed()
            })
            .collect();

        batch.send().await?;

        let filter = Filter::new().from_block(from_block).to_block(to_block);

        let mirrors_filter = crate::mirrors_filter(filter.clone());
        let router_and_wvara_filter =
            crate::router_and_wvara_filter(filter, router_address, wvara_address);

        let logs_request = future::try_join(
            provider.get_logs(&router_and_wvara_filter),
            provider.get_logs(&mirrors_filter),
        );

        let (blocks, logs) = future::join(future::join_all(headers_request), logs_request).await;
        let (router_and_wvara_logs, mirrors_logs) = logs?;

        let mut blocks_data = Vec::new();

        for response in blocks {
            let block = response?.ok_or_else(|| anyhow!("Block not found"))?;
            let block_hash = H256(block.header.hash.0);

            let header = BlockHeader {
                height: block.header.number as u32,
                timestamp: block.header.timestamp,
                parent_hash: H256(block.header.parent_hash.0),
            };

            blocks_data.push(BlockData {
                hash: block_hash,
                header,
                events: Vec::new(),
            });
        }

        let mut events = crate::logs_to_events(
            router_and_wvara_logs,
            mirrors_logs,
            router_address,
            wvara_address,
        )?;
        for block_data in blocks_data.iter_mut() {
            block_data.events = events.remove(&block_data.hash).unwrap_or_default();
        }

        Ok(blocks_data)
    }

    async fn load_block_data(&self, block: H256, header: Option<BlockHeader>) -> Result<BlockData> {
        log::trace!("Querying data for one block {block:?}");

        let filter = Filter::new().at_block_hash(block.0);
        let mirrors_filter = crate::mirrors_filter(filter.clone());
        let router_and_wvara_filter =
            crate::router_and_wvara_filter(filter, self.router_address, self.wvara_address);

        let logs_request = future::try_join(
            self.provider.get_logs(&router_and_wvara_filter),
            self.provider.get_logs(&mirrors_filter),
        );

        let ((block_hash, header), (router_and_wvara_logs, mirrors_logs)) =
            if let Some(header) = header {
                ((block, header), logs_request.await?)
            } else {
                let block_request = self.provider.get_block_by_hash(
                    block.0.into(),
                    alloy::rpc::types::BlockTransactionsKind::Hashes,
                );

                match future::try_join(block_request, logs_request).await {
                    Ok((response, logs)) => (crate::block_response_to_data(response)?, logs),
                    Err(err) => Err(err)?,
                }
            };

        if block_hash != block {
            return Err(anyhow!("Expected block hash {block}, got {block_hash}"));
        }

        let events = crate::logs_to_events(
            router_and_wvara_logs,
            mirrors_logs,
            self.router_address,
            self.wvara_address,
        )?;

        if events.len() > 1 {
            return Err(anyhow!(
                "Expected events for at most 1 block, but got for {}",
                events.len()
            ));
        }

        let (block_hash, events) = events
            .into_iter()
            .next()
            .unwrap_or_else(|| (block_hash, Vec::new()));

        if block_hash != block {
            return Err(anyhow!("Expected block hash {block}, got {block_hash}"));
        }

        Ok(BlockData {
            hash: block,
            header,
            events,
        })
    }

    async fn load_blocks_data(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<HashMap<H256, BlockData>> {
        let batch_futures: FuturesUnordered<_> = (from_block..=to_block)
            .step_by(MAX_QUERY_BLOCK_RANGE)
            .map(|start| {
                let end = (start + MAX_QUERY_BLOCK_RANGE as u64 - 1).min(to_block);

                Self::load_blocks_batch_data(
                    self.provider.clone(),
                    self.router_address,
                    self.wvara_address,
                    start,
                    end,
                )
                .boxed()
            })
            .collect();

        future::try_join_all(batch_futures).await.map(|batches| {
            batches
                .into_iter()
                .flat_map(|batch| {
                    batch
                        .into_iter()
                        .map(|block_data| (block_data.hash, block_data))
                })
                .collect()
        })
    }

    pub(crate) async fn sync(self, chain_head: Header) -> Result<(H256, Vec<(CodeId, CodeInfo)>)> {
        let latest_synced_block_height =
            self.database
                .latest_synced_block_height()
                .unwrap_or_else(|| {
                    unreachable!("latest_synced_block_height must be set in ObserverService::new")
                });

        let header = BlockHeader {
            height: chain_head.number as u32,
            timestamp: chain_head.timestamp,
            parent_hash: H256(chain_head.parent_hash.0),
        };

        let mut blocks_data = if header.height <= latest_synced_block_height {
            log::warn!(
                "Get a block with number {} <= latest synced block number: {}, maybe a reorg",
                header.height,
                latest_synced_block_height
            );
            Default::default()
        } else {
            if (header.height - latest_synced_block_height) >= self.max_sync_depth {
                // TODO (gsobol): return an event to notify about too deep chain.
                return Err(anyhow!(
                    "Too much to sync: current block number: {}, Latest valid block number: {}, Max depth: {}",
                    header.height,
                    latest_synced_block_height,
                    self.max_sync_depth
                ));
            }

            if header.height - latest_synced_block_height > self.heuristic_sync_depth {
                self.load_blocks_data(latest_synced_block_height as u64, header.height as u64)
                    .await?
            } else {
                Default::default()
            }
        };

        let mut codes_to_load_now = HashSet::new();
        let mut codes_to_load_later = HashMap::new();
        let mut chain = Vec::new();

        let mut hash = H256(chain_head.hash.0);
        while !self.database.block_is_synced(hash) {
            let block_data = match blocks_data.remove(&hash) {
                Some(data) => data,
                None => {
                    self.load_block_data(
                        hash,
                        (hash == H256(chain_head.hash.0)).then_some(header.clone()),
                    )
                    .await?
                }
            };

            if hash != block_data.hash {
                unreachable!(
                    "Expected data for block hash {hash}, got for {}",
                    block_data.hash
                );
            }

            for event in &block_data.events {
                match event {
                    BlockEvent::Router(RouterEvent::CodeValidationRequested {
                        code_id,
                        timestamp,
                        tx_hash,
                    }) => {
                        let code_info = CodeInfo {
                            timestamp: *timestamp,
                            tx_hash: *tx_hash,
                        };
                        self.database.set_code_info(*code_id, code_info.clone());

                        if !self.database.original_code_exists(*code_id)
                            && !codes_to_load_now.contains(code_id)
                        {
                            codes_to_load_later.insert(*code_id, code_info);
                        }
                    }
                    BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, .. }) => {
                        if codes_to_load_later.contains_key(code_id) {
                            return Err(anyhow!("Code {code_id} is validated before requested"));
                        };

                        if !self.database.original_code_exists(*code_id) {
                            codes_to_load_now.insert(*code_id);
                        }
                    }
                    _ => {}
                }
            }

            self.database.set_block_header(hash, &block_data.header);
            self.database.set_block_events(hash, &block_data.events);

            chain.push(hash);

            hash = block_data.header.parent_hash;
        }

        // TODO (gsobol): consider to change this behaviour of loading already validated codes.
        // Must be done with ObserverService::codes_futures together.
        // May be we should use futures_bounded::FuturesMap for this.
        let codes_futures = FuturesUnordered::new();
        for code_id in codes_to_load_now {
            let code_info = self
                .database
                .code_info(code_id)
                .ok_or_else(|| anyhow!("Code info for code {code_id} is missing"))?;

            codes_futures.push(
                crate::read_code_from_tx_hash(
                    self.blobs_reader.clone(),
                    code_id,
                    code_info.timestamp,
                    code_info.tx_hash,
                    None,
                )
                .boxed(),
            );
        }

        for res in future::join_all(codes_futures).await {
            let (code_id, _, code) = res?;
            self.database.set_original_code(code_id, code.as_slice());
        }

        for hash in chain.iter().rev() {
            let block_header = self
                .database
                .block_header(*hash)
                .unwrap_or_else(|| unreachable!("Block header for synced block {hash} is missing"));

            // Setting block as synced means: all on-chain data for this block is loaded and at least all positive validated codes are loaded.
            self.database.set_block_is_synced(*hash);

            self.database
                .set_latest_synced_block_height(block_header.height);
        }

        Ok((
            chain_head.hash.0.into(),
            codes_to_load_later.into_iter().collect(),
        ))
    }
}

#![allow(unused)]

use crate::{
    event,
    observer::{read_block_events, read_block_request_events, read_code_from_tx_hash},
    BlobReader, BlockData, ConsensusLayerBlobReader, ObserverStatus, RequestBlockData,
};
use alloy::{
    primitives::Address as AlloyAddress,
    providers::{Provider as _, ProviderBuilder, RootProvider},
    pubsub::{Subscription, SubscriptionStream},
    rpc::types::eth::Header,
    signers::k256::elliptic_curve::rand_core::block,
    transports::BoxTransport,
};
use anyhow::{anyhow, Context, Result};
use ethexe_db::BlockHeader;
use ethexe_signer::Address;
use futures::{
    future::{self, BoxFuture},
    pin_mut,
    stream::{self, FuturesUnordered},
    Stream, StreamExt,
};
use gprimitives::{CodeId, H256};
use std::{collections::VecDeque, future::Future, mem, pin::Pin, sync::Arc, time::Duration};
use tokio::{select, sync::mpsc, task::JoinHandle};

type Provider = RootProvider<BoxTransport>;

pub struct EthereumConfig {
    pub rpc: String,
    pub beacon_rpc: String,
    pub router_address: Address,
    pub block_time: Duration,
}

pub struct Service {
    blobs: Arc<dyn BlobReader>,
    provider: Provider,

    router: Address,

    blocks_stream: SubscriptionStream<Header>,
    codes_futures: FuturesUnordered<BlobDownloadFuture>,
}

type BlobDownloadFuture = BoxFuture<'static, Result<(CodeId, Vec<u8>)>>;

impl Service {
    pub async fn new(config: &EthereumConfig) -> Result<Self> {
        let blobs = Arc::new(
            ConsensusLayerBlobReader::new(&config.rpc, &config.beacon_rpc, config.block_time)
                .await
                .context("failed to create blob reader")?,
        );

        Self::new_with_blobs(config, blobs).await
    }

    pub async fn new_with_blobs(
        config: &EthereumConfig,
        blobs: Arc<dyn BlobReader>,
    ) -> Result<Self> {
        let provider = ProviderBuilder::new()
            .on_builtin(&config.rpc)
            .await
            .context("failed to create ethereum provider")?;

        let subscription = provider
            .subscribe_blocks()
            .await
            .context("failed to subscribe blocks")?;

        let blocks_stream = subscription.into_stream();

        Ok(Self {
            blobs,
            provider,
            router: config.router_address,
            blocks_stream,
            codes_futures: FuturesUnordered::new(),
        })
    }

    pub async fn next(&mut self) -> Result<Event> {
        select! {
            header = self.blocks_stream.next() => {
                let header = header.ok_or_else(|| anyhow!("blocks stream closed"))?;

                let block_hash = (*header.hash).into();
                let parent_hash = (*header.parent_hash).into();
                let block_number = header.number as u32;
                let block_timestamp = header.timestamp;

                log::trace!("Received block: {block_hash:?}");

                let header = BlockHeader {
                    height: block_number,
                    timestamp: block_timestamp,
                    parent_hash,
                };

                read_block_request_events(block_hash, &self.provider, self.router.0.into())
                    .await
                    .map(|events| Event::Block(RequestBlockData {
                        hash: block_hash,
                        header,
                        events,
                    }))
                    .map_err(|err| anyhow!("Failed to read events for block {block_hash:?}: {err}"))
            },
            res = self.codes_futures.select_next_some() => {
                res.map(|(code_id, code)| Event::Blob { code_id, code })
            }
        }
    }
}

pub enum Event {
    Blob { code_id: CodeId, code: Vec<u8> },
    Block(RequestBlockData),
    Status(ObserverStatus),
}

pub struct EventReceiver(mpsc::UnboundedReceiver<Event>);

impl EventReceiver {
    pub async fn recv(&mut self) -> Result<Event> {
        self.0
            .recv()
            .await
            .ok_or_else(|| anyhow!("connection closed"))
    }
}

pub enum Request {
    LookupBlob { code_id: CodeId, blob_tx_hash: H256 },
    SyncStatus,
}

#[derive(Clone)]
pub struct RequestSender(mpsc::UnboundedSender<Request>);

impl RequestSender {
    fn send_request(&self, request: Request) -> Result<()> {
        self.0.send(request).map_err(|_| anyhow!("service is down"))
    }

    pub fn lookup_blob(&self, code_id: CodeId, blob_tx_hash: H256) -> Result<()> {
        self.send_request(Request::LookupBlob {
            code_id,
            blob_tx_hash,
        })
    }

    pub fn sync_status(&self) -> Result<()> {
        self.send_request(Request::SyncStatus)
    }
}

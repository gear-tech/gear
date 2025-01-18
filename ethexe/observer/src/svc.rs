#![allow(unused)]

use crate::{
    observer::{read_block_events, read_block_request_events, read_code_from_tx_hash},
    BlobReader, BlockData, ConsensusLayerBlobReader, ObserverStatus, RequestBlockData,
};
use alloy::{
    primitives::Address as AlloyAddress,
    providers::{Provider as _, ProviderBuilder, RootProvider},
    pubsub::Subscription,
    rpc::types::eth::Header,
    signers::k256::elliptic_curve::rand_core::block,
    transports::BoxTransport,
};
use anyhow::{anyhow, Context, Result};
use ethexe_db::BlockHeader;
use ethexe_signer::Address;
use futures::{Stream, StreamExt};
use gprimitives::{CodeId, H256};
use std::{future::Future, mem, sync::Arc, time::Duration};
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
    subscription: Subscription<Header>,

    router: Address,
}

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

        Ok(Self {
            blobs,
            provider,
            subscription,
            router: config.router_address,
        })
    }

    // TODO: move me to static method with first argument: Subscription<Header>.
    fn stream<E, EF, BD, RE, BDC>(
        &mut self,
        read_events: RE,
        block_data_constructor: BDC,
    ) -> impl Stream<Item = BD> + use<'_, E, EF, BD, RE, BDC>
    where
        EF: Future<Output = Result<E>>,
        RE: Fn(Provider, Address, H256) -> EF,
        BDC: Fn(H256, BlockHeader, E) -> BD,
    {
        let new_subscription = self.subscription.resubscribe();
        let old_subscription = mem::replace(&mut self.subscription, new_subscription);

        let mut headers_stream = old_subscription.into_stream();

        async_stream::stream! {
            while let Some(header) = headers_stream.next().await {
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

                match read_events(self.provider.clone(), self.router, block_hash).await {
                    Ok(events) => {
                        yield block_data_constructor(block_hash, header, events);
                    },
                    Err(err) => {
                        log::error!("Failed to read events for block {block_hash:?}: {err}");
                        continue;
                    },
                };
            }
        }
    }

    pub fn block_stream(&mut self) -> impl Stream<Item = BlockData> + '_ {
        self.stream(
            |provider, router, block_hash| async move {
                read_block_events(block_hash, &provider, router.0.into()).await
            },
            |block_hash, header, events| BlockData {
                hash: block_hash,
                header,
                events,
            },
        )
    }

    pub fn request_block_stream(&mut self) -> impl Stream<Item = RequestBlockData> + '_ {
        self.stream(
            |provider, router, block_hash| async move {
                read_block_request_events(block_hash, &provider, router.0.into()).await
            },
            |block_hash, header, events| RequestBlockData {
                hash: block_hash,
                header,
                events,
            },
        )
    }

    pub fn run(mut self) -> (JoinHandle<Result<()>>, RequestSender, EventReceiver) {
        let (request_tx, mut request_rx) = mpsc::unbounded_channel();
        let (mut event_tx, event_rx) = mpsc::unbounded_channel();

        let handle = tokio::spawn(async move {
            let blob_reader = self.blobs.clone();

            let blocks_stream = self.request_block_stream();
            futures::pin_mut!(blocks_stream);

            let mut status = ObserverStatus::default();

            loop {
                tokio::select! {
                    block = blocks_stream.next() => {
                        let block = block.ok_or_else(|| anyhow!("block stream closed"))?;

                        status.eth_block_number = block.header.height as u64;

                        event_tx.send(Event::Block(block)).map_err(|_| anyhow!("failed to send block data event"))?;
                    },
                    request = request_rx.recv() => {
                        let request = request.ok_or_else(|| anyhow!("failed to receive request: channel closed"))?;

                        match request {
                            Request::LookupBlob { code_id, blob_tx_hash } => {
                                let attempts = Some(3);

                                let res = read_code_from_tx_hash(blob_reader.clone(), code_id, blob_tx_hash, attempts).await;

                                status.pending_upload_code += 1;

                                if let Ok((code_id, code)) = res.inspect_err(|e| log::error!("failed to handle upload code event: {e}"))  {
                                    event_tx.send(Event::Blob { code_id, code }).map_err(|_| anyhow!("failed to send status event"))?
                                }
                            },
                            Request::SyncStatus => {
                                let status = mem::take(&mut status);

                                event_tx.send(Event::Status(status)).map_err(|_| anyhow!("failed to send status event"))?;
                            },
                        }
                    }
                }
            }

            Ok(())
        });

        (handle, RequestSender(request_tx), EventReceiver(event_rx))
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

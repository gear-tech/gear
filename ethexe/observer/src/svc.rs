#![allow(unused)]

use crate::{
    event,
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
    subscription: Subscription<Header>,

    router: Address,

    request_block_stream: Option<Pin<Box<dyn Stream<Item = RequestBlockData>>>>,
    #[allow(clippy::type_complexity)]
    codes_futures: FuturesUnordered<BoxFuture<'static, Result<(CodeId, Vec<u8>)>>>,
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
            request_block_stream: None,
            codes_futures: FuturesUnordered::new(),
        })
    }

    fn request_events_raw(
        provider: Provider,
        router: Address,
        sub: Subscription<Header>,
    ) -> impl Stream<Item = RequestBlockData> {
        let mut headers_stream = sub.into_stream();

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

                match read_block_request_events(block_hash, &provider, router.0.into()).await {
                    Ok(events) => {
                        yield RequestBlockData {
                            hash: block_hash,
                            header,
                            events,
                        }
                    },
                    Err(err) => {
                        log::error!("Failed to read events for block {block_hash:?}: {err}");
                        continue;
                    },
                };
            }
        }
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

    // pub fn request_block_stream<'a>(&mut self) -> impl Stream<Item = RequestBlockData> + 'a {
    //     self.stream(
    //         move |provider, router, block_hash| async move {
    //             read_block_request_events(block_hash, &provider, router.0.into()).await
    //         },
    //         move |block_hash, header, events| RequestBlockData {
    //             hash: block_hash,
    //             header,
    //             events,
    //         },
    //     )
    // }

    pub fn set_blocks_stream(&mut self) {
        self.request_block_stream = Some(Box::pin(Self::request_events_raw(
            self.provider.clone(),
            self.router,
            self.subscription.resubscribe(),
        )));
    }

    pub async fn next(&mut self) -> Result<Event> {
        if self.request_block_stream.is_none() {
            self.set_blocks_stream();
        }

        select! {
            block = self.request_block_stream.as_mut().expect("infallible").next() => {
                let block = block.ok_or_else(|| anyhow!("block stream closed"))?;

                // status.eth_block_number = block.header.height as u64;

                Ok(Event::Block(block))
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

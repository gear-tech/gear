use crate::{
    event::{BlockData, BlockDataForHandling, Event, EventForHandling},
    BlobReader,
};
use alloy::{
    primitives::Address as AlloyAddress,
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::types::eth::{Filter, Topic},
    transports::BoxTransport,
};
use anyhow::{anyhow, Result};
use ethexe_common::{
    router::{Event as RouterEvent, EventForHandling as RouterEventForHandling},
    BlockEvent, BlockEventForHandling,
};
use ethexe_ethereum::{
    mirror,
    router::{self, RouterQuery},
    wvara,
};
use ethexe_signer::Address;
use futures::{future, stream::FuturesUnordered, Stream, StreamExt};
use gear_core::ids::prelude::*;
use gprimitives::{ActorId, CodeId, H256};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::watch;

/// Max number of blocks to query in alloy.
pub(crate) const MAX_QUERY_BLOCK_RANGE: u64 = 100_000;

pub(crate) type ObserverProvider = RootProvider<BoxTransport>;

#[derive(Clone)]
pub struct Observer {
    provider: ObserverProvider,
    router_address: AlloyAddress,
    blob_reader: Arc<dyn BlobReader>,
    status_sender: watch::Sender<ObserverStatus>,
    status: ObserverStatus,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ObserverStatus {
    pub eth_block_number: u64,
    pub pending_upload_code: u64,
    pub last_router_state: u64,
}

impl Observer {
    pub async fn new(
        ethereum_rpc: &str,
        router_address: Address,
        blob_reader: Arc<dyn BlobReader>,
    ) -> Result<Self> {
        let (status_sender, _status_receiver) = watch::channel(ObserverStatus::default());
        Ok(Self {
            provider: ProviderBuilder::new().on_builtin(ethereum_rpc).await?,
            router_address: AlloyAddress::new(router_address.0),
            blob_reader,
            status: Default::default(),
            status_sender,
        })
    }

    pub fn get_status_receiver(&self) -> watch::Receiver<ObserverStatus> {
        self.status_sender.subscribe()
    }

    fn update_status<F>(&mut self, update_fn: F)
    where
        F: FnOnce(&mut ObserverStatus),
    {
        update_fn(&mut self.status);
        let _ = self.status_sender.send_replace(self.status);
    }
    pub fn provider(&self) -> &ObserverProvider {
        &self.provider
    }

    pub fn events_all(&mut self) -> impl Stream<Item = Event> + '_ {
        async_stream::stream! {
            let block_subscription = self
                .provider
                .subscribe_blocks()
                .await
                .expect("failed to subscribe to blocks");
            let mut block_stream = block_subscription.into_stream();
            let mut futures = FuturesUnordered::new();

            loop {
                tokio::select! {
                    block = block_stream.next() => {
                        let Some(block) = block else {
                            log::info!("Block stream ended");
                            break;
                        };

                        log::trace!("Received block: {:?}", block.header.hash);

                        let block_hash = (*block.header.hash.expect("failed to get block hash")).into();
                        let parent_hash = (*block.header.parent_hash).into();
                        let block_number = block.header.number.expect("failed to get block number");
                        let block_timestamp = block.header.timestamp;

                        let events = match read_block_events(block_hash, &self.provider, self.router_address).await {
                            Ok(events) => events,
                            Err(err) => {
                                log::error!("failed to read events: {err}");
                                continue;
                            }
                        };

                        let mut codes_len = 0;

                        // Create futures to load codes
                        for event in events.iter() {
                            if let BlockEvent::Router(RouterEvent::CodeValidationRequested { code_id, blob_tx_hash }) = event {
                                codes_len += 1;

                                let blob_reader = self.blob_reader.clone();

                                let code_id = *code_id;
                                let blob_tx_hash = *blob_tx_hash;

                                futures.push(async move {
                                    let attempts = Some(3);

                                    read_code_from_tx_hash(
                                        blob_reader,
                                        code_id,
                                        blob_tx_hash,
                                        attempts,
                                    ).await
                                });
                            }
                        }

                        self.update_status(|status| {
                            status.eth_block_number = block_number;
                            if codes_len > 0 {
                                status.last_router_state = block_number;
                            }
                            status.pending_upload_code = codes_len as u64;
                        });

                        let block_data = BlockData {
                            block_hash,
                            parent_hash,
                            block_number,
                            block_timestamp,
                            events,
                        };

                        yield Event::Block(block_data);
                    },
                    future = futures.next(), if !futures.is_empty() => {
                        match future {
                            Some(Ok((code_id, code))) => yield Event::CodeLoaded { code_id, code },
                            Some(Err(err)) => log::error!("failed to handle upload code event: {err}"),
                            None => continue,
                        }
                    }
                };
            }
        }
    }

    pub fn events_for_handling(&mut self) -> impl Stream<Item = EventForHandling> + '_ {
        async_stream::stream! {
            let block_subscription = self
                .provider
                .subscribe_blocks()
                .await
                .expect("failed to subscribe to blocks");
            let mut block_stream = block_subscription.into_stream();
            let mut futures = FuturesUnordered::new();

            loop {
                tokio::select! {
                    block = block_stream.next() => {
                        let Some(block) = block else {
                            log::info!("Block stream ended");
                            break;
                        };

                        log::trace!("Received block: {:?}", block.header.hash);

                        let block_hash = (*block.header.hash.expect("failed to get block hash")).into();
                        let parent_hash = (*block.header.parent_hash).into();
                        let block_number = block.header.number.expect("failed to get block number");
                        let block_timestamp = block.header.timestamp;

                        let events = match read_block_events_for_handling(block_hash, &self.provider, self.router_address).await {
                            Ok(events) => events,
                            Err(err) => {
                                log::error!("failed to read events: {err}");
                                continue;
                            }
                        };

                        let mut codes_len = 0;

                        // Create futures to load codes
                        // TODO (breathx): remove me from here mb
                        for event in events.iter() {
                            if let BlockEventForHandling::Router(RouterEventForHandling::CodeValidationRequested { code_id, blob_tx_hash }) = event {
                                codes_len += 1;

                                let blob_reader = self.blob_reader.clone();

                                let code_id = *code_id;
                                let blob_tx_hash = *blob_tx_hash;

                                futures.push(async move {
                                    let attempts = Some(3);

                                    read_code_from_tx_hash(
                                        blob_reader,
                                        code_id,
                                        blob_tx_hash,
                                        attempts,
                                    ).await
                                });
                            }
                        }

                        self.update_status(|status| {
                            status.eth_block_number = block_number;
                            if codes_len > 0 {
                                status.last_router_state = block_number;
                            }
                            status.pending_upload_code = codes_len as u64;
                        });

                        let block_data = BlockDataForHandling {
                            block_hash,
                            parent_hash,
                            block_number,
                            block_timestamp,
                            events,
                        };

                        yield EventForHandling::Block(block_data);
                    },
                    future = futures.next(), if !futures.is_empty() => {
                        match future {
                            Some(Ok((code_id, code))) => yield EventForHandling::CodeLoaded { code_id, code },
                            Some(Err(err)) => log::error!("failed to handle upload code event: {err}"),
                            None => continue,
                        }
                    }
                };
            }
        }
    }
}

pub(crate) async fn read_code_from_tx_hash(
    blob_reader: Arc<dyn BlobReader>,
    expected_code_id: CodeId,
    tx_hash: H256,
    attempts: Option<u8>,
) -> Result<(CodeId, Vec<u8>)> {
    let code = blob_reader
        .read_blob_from_tx_hash(tx_hash, attempts)
        .await
        .map_err(|err| anyhow!("failed to read blob: {err}"))?;

    (CodeId::generate(&code) == expected_code_id)
        .then_some(())
        .ok_or_else(|| anyhow!("unexpected code id"))?;

    Ok((expected_code_id, code))
}

// TODO (breathx): only read events that require some activity.
// TODO (breathx): don't store not our events.
#[allow(unused)] // TODO (breathx).
pub(crate) async fn read_block_events(
    block_hash: H256,
    provider: &ObserverProvider,
    router_address: AlloyAddress,
) -> Result<Vec<BlockEvent>> {
    let router_query = RouterQuery::from_provider(router_address, Arc::new(provider.clone()));
    let wvara_address = router_query.wvara_address().await?;

    let filter = Filter::new().at_block_hash(block_hash.to_fixed_bytes());

    read_events_impl(router_address, wvara_address, provider, filter)
        .await
        .map(|v| v.into_values().next().unwrap_or_default())
}

#[allow(unused)] // TODO (breathx)
pub(crate) async fn read_block_events_batch(
    from_block: u32,
    to_block: u32,
    provider: &ObserverProvider,
    router_address: AlloyAddress,
) -> Result<HashMap<H256, Vec<BlockEvent>>> {
    let router_query = RouterQuery::from_provider(router_address, Arc::new(provider.clone()));
    let wvara_address = router_query.wvara_address().await?;

    let mut res = HashMap::new();

    let mut start_block = from_block as u64;
    let to_block = to_block as u64;

    while start_block <= to_block {
        let end_block = to_block.min(start_block + MAX_QUERY_BLOCK_RANGE - 1);

        let filter = Filter::new().from_block(start_block).to_block(end_block);

        let iter_res = read_events_impl(router_address, wvara_address, provider, filter).await?;

        res.extend(iter_res.into_iter());

        start_block = end_block + 1;
    }

    Ok(res)
}

async fn read_events_impl(
    router_address: AlloyAddress,
    wvara_address: AlloyAddress,
    provider: &ObserverProvider,
    filter: Filter,
) -> Result<HashMap<H256, Vec<BlockEvent>>> {
    let router_and_wvara_topic = Topic::from_iter(
        router::events::signatures::ALL
            .iter()
            .chain(wvara::events::signatures::ALL)
            .cloned(),
    );

    let router_and_wvara_filter = filter
        .clone()
        .address(vec![router_address, wvara_address])
        .event_signature(router_and_wvara_topic);

    let mirror_filter = filter.event_signature(Topic::from_iter(
        mirror::events::signatures::ALL.iter().cloned(),
    ));

    let (router_and_wvara_logs, mirrors_logs) = future::try_join(
        provider.get_logs(&router_and_wvara_filter),
        provider.get_logs(&mirror_filter),
    )
    .await?;

    let block_hash_of = |log: &alloy::rpc::types::Log| -> Result<H256> {
        log.block_hash
            .map(|v| v.0.into())
            .ok_or(anyhow!("Block hash is missing"))
    };

    let mut res: HashMap<_, Vec<_>> = HashMap::new();

    for router_or_wvara_log in router_and_wvara_logs {
        let block_hash = block_hash_of(&router_or_wvara_log)?;

        let maybe_block_event = if router_or_wvara_log.address() == router_address {
            router::events::try_extract_event(&router_or_wvara_log)?.map(Into::into)
        } else {
            wvara::events::try_extract_event(&router_or_wvara_log)?.map(Into::into)
        };

        if let Some(block_event) = maybe_block_event {
            res.entry(block_hash).or_default().push(block_event);
        }
    }

    for mirror_log in mirrors_logs {
        let block_hash = block_hash_of(&mirror_log)?;

        let address = (*mirror_log.address().into_word()).into();

        // TODO (breathx): if address is unknown, then continue.

        if let Some(event) = mirror::events::try_extract_event(&mirror_log)? {
            res.entry(block_hash)
                .or_default()
                .push(BlockEvent::mirror(address, event));
        }
    }

    Ok(res)
}

// TODO (breathx): only read events that require some activity.
// TODO (breathx): don't store not our events.
pub(crate) async fn read_block_events_for_handling(
    block_hash: H256,
    provider: &ObserverProvider,
    router_address: AlloyAddress,
) -> Result<Vec<BlockEventForHandling>> {
    let router_query = RouterQuery::from_provider(router_address, Arc::new(provider.clone()));
    let wvara_address = router_query.wvara_address().await?;

    let filter = Filter::new().at_block_hash(block_hash.to_fixed_bytes());

    read_events_for_handling_impl(router_address, wvara_address, provider, filter)
        .await
        .map(|v| v.into_values().next().unwrap_or_default())
}

pub(crate) async fn read_block_events_for_handling_batch(
    from_block: u32,
    to_block: u32,
    provider: &ObserverProvider,
    router_address: AlloyAddress,
) -> Result<HashMap<H256, Vec<BlockEventForHandling>>> {
    let router_query = RouterQuery::from_provider(router_address, Arc::new(provider.clone()));
    let wvara_address = router_query.wvara_address().await?;

    let mut res = HashMap::new();

    let mut start_block = from_block as u64;
    let to_block = to_block as u64;

    while start_block <= to_block {
        let end_block = to_block.min(start_block + MAX_QUERY_BLOCK_RANGE - 1);

        let filter = Filter::new().from_block(start_block).to_block(end_block);

        let iter_res =
            read_events_for_handling_impl(router_address, wvara_address, provider, filter).await?;

        res.extend(iter_res.into_iter());

        start_block = end_block + 1;
    }

    Ok(res)
}

async fn read_events_for_handling_impl(
    router_address: AlloyAddress,
    wvara_address: AlloyAddress,
    provider: &ObserverProvider,
    filter: Filter,
) -> Result<HashMap<H256, Vec<BlockEventForHandling>>> {
    let router_and_wvara_topic = Topic::from_iter(
        router::events::signatures::FOR_HANDLING
            .iter()
            .chain(wvara::events::signatures::FOR_HANDLING)
            .cloned(),
    );

    let router_and_wvara_filter = filter
        .clone()
        .address(vec![router_address, wvara_address])
        .event_signature(router_and_wvara_topic);

    let mirror_filter = filter.event_signature(Topic::from_iter(
        mirror::events::signatures::FOR_HANDLING.iter().cloned(),
    ));

    let (router_and_wvara_logs, mirrors_logs) = future::try_join(
        provider.get_logs(&router_and_wvara_filter),
        provider.get_logs(&mirror_filter),
    )
    .await?;

    let block_hash_of = |log: &alloy::rpc::types::Log| -> Result<H256> {
        log.block_hash
            .map(|v| v.0.into())
            .ok_or(anyhow!("Block hash is missing"))
    };

    let out_of_scope_addresses = [
        (*router_address.into_word()).into(),
        (*wvara_address.into_word()).into(),
        ActorId::zero(),
    ];

    let mut res: HashMap<_, Vec<_>> = HashMap::new();

    for router_or_wvara_log in router_and_wvara_logs {
        let block_hash = block_hash_of(&router_or_wvara_log)?;

        let maybe_block_event_for_handling = if router_or_wvara_log.address() == router_address {
            router::events::try_extract_event_for_handling(&router_or_wvara_log)?.map(Into::into)
        } else {
            wvara::events::try_extract_event_for_handling(&router_or_wvara_log)?
                .filter(|v| !v.involves_addresses(&out_of_scope_addresses))
                .map(Into::into)
        };

        if let Some(block_event_for_handling) = maybe_block_event_for_handling {
            res.entry(block_hash)
                .or_default()
                .push(block_event_for_handling);
        }
    }

    for mirror_log in mirrors_logs {
        let block_hash = block_hash_of(&mirror_log)?;

        let address = (*mirror_log.address().into_word()).into();

        // TODO (breathx): if address is unknown, then continue.

        if let Some(event_for_handling) =
            mirror::events::try_extract_event_for_handling(&mirror_log)?
        {
            res.entry(block_hash)
                .or_default()
                .push(BlockEventForHandling::mirror(address, event_for_handling));
        }
    }

    Ok(res)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::MockBlobReader;
    use alloy::node_bindings::Anvil;
    use ethexe_ethereum::Ethereum;
    use ethexe_signer::Signer;
    use tokio::{sync::oneshot, task};

    fn wat2wasm_with_validate(s: &str, validate: bool) -> Vec<u8> {
        wabt::Wat2Wasm::new()
            .validate(validate)
            .convert(s)
            .unwrap()
            .as_ref()
            .to_vec()
    }

    fn wat2wasm(s: &str) -> Vec<u8> {
        wat2wasm_with_validate(s, true)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_deployment() -> Result<()> {
        gear_utils::init_default_logger();

        let mut anvil = Anvil::new().try_spawn()?;
        drop(anvil.child_mut().stdout.take()); //temp fix for alloy#1078

        let ethereum_rpc = anvil.ws_endpoint();

        let signer = Signer::new("/tmp/keys".into())?;

        let sender_public_key = signer.add_key(
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".parse()?,
        )?;
        let sender_address = sender_public_key.to_address();
        let validators = vec!["0x45D6536E3D4AdC8f4e13c5c4aA54bE968C55Abf1".parse()?];

        let ethereum = Ethereum::deploy(&ethereum_rpc, validators, signer, sender_address).await?;
        let blob_reader = Arc::new(MockBlobReader::new(Duration::from_secs(1)));

        let router_address = ethereum.router().address();
        let cloned_blob_reader = blob_reader.clone();

        let (send_subscription_created, receive_subscription_created) = oneshot::channel::<()>();
        let handle = task::spawn(async move {
            let mut observer = Observer::new(&ethereum_rpc, router_address, cloned_blob_reader)
                .await
                .expect("failed to create observer");

            let observer_events = observer.events_all();
            futures::pin_mut!(observer_events);

            send_subscription_created.send(()).unwrap();

            while let Some(event) = observer_events.next().await {
                if matches!(event, Event::CodeLoaded { .. }) {
                    return Some(event);
                }
            }

            None
        });
        receive_subscription_created.await.unwrap();

        let wat = r#"
            (module
                (import "env" "memory" (memory 0))
                (export "init" (func $init))
                (func $init)
            )
        "#;
        let wasm = wat2wasm(wat);

        let code_id = CodeId::generate(&wasm);
        let blob_tx = H256::random();

        blob_reader.add_blob_transaction(blob_tx, wasm).await;
        ethereum
            .router()
            .request_code_validation(code_id, blob_tx)
            .await?;

        assert!(
            handle.await?.is_some(),
            "observer did not receive upload code event"
        );

        Ok(())
    }
}

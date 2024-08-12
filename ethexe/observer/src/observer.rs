use crate::{BlobReader, BlockData, Event};
use alloy::{
    primitives::{Address as AlloyAddress, B256},
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::types::eth::{Filter, Topic},
    transports::BoxTransport,
};
use anyhow::{anyhow, Result};
use ethexe_common::{router::Event as RouterEvent, BlockEvent};
use ethexe_ethereum::{mirror, router};
use ethexe_signer::Address;
use futures::{future, stream::FuturesUnordered, Stream, StreamExt};
use gear_core::ids::prelude::*;
use gprimitives::{CodeId, H256};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::watch;

/// Max number of blocks to query in alloy.
pub(crate) const MAX_QUERY_BLOCK_RANGE: u32 = 100_000;

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

    pub fn events(&mut self) -> impl Stream<Item = Event> + '_ {
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

                                let code_id = code_id.clone();
                                let blob_tx_hash = blob_tx_hash.clone();

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
// TODO (breathx): read WVara events.
pub(crate) async fn read_block_events(
    block_hash: H256,
    provider: &ObserverProvider,
    router_address: AlloyAddress,
) -> Result<Vec<BlockEvent>> {
    let router_events_filter = Filter::new()
        .at_block_hash(block_hash.0)
        .address(router_address)
        .event_signature(Topic::from_iter(
            router::events::signatures::ALL
                .iter()
                .map(|hash| B256::new(hash.to_fixed_bytes())),
        ));

    let router_logs_fut = provider.get_logs(&router_events_filter);

    let mirrors_events_filter =
        Filter::new()
            .at_block_hash(block_hash.0)
            .event_signature(Topic::from_iter(
                mirror::events::signatures::ALL
                    .iter()
                    .map(|hash| B256::new(hash.to_fixed_bytes())),
            ));

    let mirrors_logs_fut = provider.get_logs(&mirrors_events_filter);

    let (router_logs, mirrors_logs) = future::join(router_logs_fut, mirrors_logs_fut).await;
    let (router_logs, mirrors_logs) = (router_logs?, mirrors_logs?);

    let mut block_events = Vec::with_capacity(router_logs.len() + mirrors_logs.len());

    for router_log in router_logs {
        let Some(router_event) = router::events::try_extract_event(router_log)? else {
            continue;
        };

        block_events.push(router_event.into())
    }

    for mirror_log in mirrors_logs {
        let address = (*mirror_log.address().into_word()).into();

        let Some(mirror_event) = mirror::events::try_extract_event(mirror_log)? else {
            continue;
        };

        block_events.push(BlockEvent::mirror(address, mirror_event));
    }

    Ok(block_events)
}

#[allow(unused)]
pub(crate) async fn read_block_events_batch(
    from_block: u32,
    to_block: u32,
    provider: &ObserverProvider,
    router_address: AlloyAddress,
) -> Result<BTreeMap<H256, Vec<BlockEvent>>> {
    let _ = MAX_QUERY_BLOCK_RANGE;
    todo!("TODO (breathx)")
    // let mut events_map: BTreeMap<H256, Vec<BlockEvent>> = BTreeMap::new();
    // let mut start_block = from_block;

    // while start_block <= to_block {
    //     let end_block = std::cmp::min(start_block + MAX_QUERY_BLOCK_RANGE - 1, to_block);
    //     let router_events_filter = Filter::new()
    //         .from_block(start_block as u64)
    //         .to_block(end_block as u64)
    //         .address(router_address)
    //         .event_signature(Topic::from_iter(
    //             signature_hash::ROUTER_EVENTS
    //                 .iter()
    //                 .map(|hash| B256::new(*hash)),
    //         ));

    //     let logs = provider.get_logs(&router_events_filter).await?;

    //     for log in logs.iter() {
    //         let block_hash = H256(log.block_hash.ok_or(anyhow!("Block hash is missing"))?.0);

    //         let Some(event) = match_log(log)? else {
    //             continue;
    //         };

    //         events_map
    //             .entry(block_hash)
    //             .and_modify(|events| events.push(event.clone()))
    //             .or_insert(vec![event]);
    //     }
    //     start_block = end_block + 1;
    // }

    // Ok(events_map)
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

            let observer_events = observer.events();
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

use crate::{BlobReader, BlockData, CodeLoadedData, Event};
use alloy::{
    primitives::{Address as AlloyAddress, B256},
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::types::eth::{Filter, Topic},
    transports::BoxTransport,
};
use anyhow::{anyhow, Result};
use ethexe_common::events::BlockEvent;
use ethexe_ethereum::event::*;
use ethexe_signer::Address;
use futures::{stream::FuturesUnordered, Stream, StreamExt};
use gear_core::ids::prelude::*;
use gprimitives::{ActorId, CodeId, H256};
use std::sync::Arc;
use tokio::sync::watch;

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

                        let block_header = block.header;
                        let block_hash = block_header.hash.expect("failed to get block hash");
                        let parent_hash = block_header.parent_hash;
                        let block_number = block_header.number.expect("failed to get block number");
                        let timestamp = block_header.timestamp;

                        let events = match read_block_events(H256(block_hash.0), &self.provider, self.router_address).await {
                            Ok(events) => events,
                            Err(err) => {
                                log::error!("failed to read events: {err}");
                                continue;
                            }
                        };

                        let mut codes_len = 0;

                        // Create futures to load codes
                        for event in events.iter() {
                            let BlockEvent::UploadCode(pending_upload_code) = event else {
                                continue
                            };

                            codes_len += 1;

                            let blob_reader = self.blob_reader.clone();
                            let origin = pending_upload_code.origin;
                            let tx_hash = pending_upload_code.blob_tx();
                            let attempts = Some(3);
                            let code_id = pending_upload_code.code_id;

                            futures.push(async move {
                                read_code_from_tx_hash(
                                    blob_reader,
                                    origin,
                                    tx_hash,
                                    attempts,
                                    code_id,
                                ).await
                            });
                        }

                        self.update_status(|status| {
                            status.eth_block_number = block_number;
                            if codes_len > 0 {
                                status.last_router_state = block_number;
                            }
                            status.pending_upload_code = codes_len as u64;
                        });

                        let block_data = BlockData {
                            block_hash: H256(block_hash.0),
                            parent_hash: H256(parent_hash.0),
                            block_number,
                            block_timestamp: timestamp,
                            events,
                        };

                        yield Event::Block(block_data);
                    },
                    future = futures.next(), if !futures.is_empty() => {
                        match future {
                            Some(future) => {
                                match future {
                                    Ok((origin, code_id, code)) => {
                                        yield Event::CodeLoaded(CodeLoadedData { origin, code_id, code });
                                    },
                                    Err(err) => log::error!("failed to handle upload code event: {err}"),
                                }
                            },
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
    origin: ActorId,
    tx_hash: H256,
    attempts: Option<u8>,
    expected_code_id: CodeId,
) -> Result<(ActorId, CodeId, Vec<u8>)> {
    let code = blob_reader
        .read_blob_from_tx_hash(tx_hash, attempts)
        .await
        .map_err(|err| anyhow!("failed to read blob: {err}"))?;

    (CodeId::generate(&code) == expected_code_id)
        .then_some(())
        .ok_or_else(|| anyhow!("unexpected code id"))?;

    Ok((origin, expected_code_id, code))
}

pub(crate) async fn read_block_events(
    block_hash: H256,
    provider: &ObserverProvider,
    router_address: AlloyAddress,
) -> Result<Vec<BlockEvent>> {
    let router_events_filter = Filter::new()
        .at_block_hash(block_hash.0)
        .address(router_address)
        .event_signature(Topic::from_iter(
            signature_hash::ROUTER_EVENTS
                .iter()
                .map(|hash| B256::new(*hash)),
        ));

    let logs = provider.get_logs(&router_events_filter).await?;

    let mut events = vec![];
    for log in logs.iter() {
        let Some(event) = match_log(log)? else {
            continue;
        };
        events.push(event);
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::MockBlobReader;
    use alloy::node_bindings::Anvil;
    use ethexe_ethereum::Ethereum;
    use ethexe_signer::Signer;
    use tokio::task;

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

        let anvil = Anvil::new().try_spawn()?;
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

        let handle = task::spawn(async move {
            let mut observer = Observer::new(&ethereum_rpc, router_address, cloned_blob_reader)
                .await
                .expect("failed to create observer");

            let observer_events = observer.events();
            futures::pin_mut!(observer_events);

            while let Some(event) = observer_events.next().await {
                if matches!(event, Event::CodeLoaded { .. }) {
                    return Some(event);
                }
            }

            None
        });

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
        ethereum.router().upload_code(code_id, blob_tx).await?;

        assert!(
            handle.await?.is_some(),
            "observer did not receive upload code event"
        );

        Ok(())
    }
}

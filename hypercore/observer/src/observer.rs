use crate::{
    event::{
        BlockEventData, ClaimValue, CodeApproved, CodeRejected, CreateProgram, SendMessage,
        SendReply, UpdatedProgram, UploadCode, UploadCodeData, UserMessageSent, UserReplySent,
    },
    BlobReader, BlockEvent, Event,
};
use alloy::{
    primitives::{Address, B256},
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::types::eth::{Filter, Topic},
    transports::BoxTransport,
};
use anyhow::{anyhow, Result};
use futures::{stream::FuturesUnordered, Stream, StreamExt};
use gear_core::ids::prelude::*;
use gprimitives::{ActorId, CodeId, H256};
use hypercore_ethereum::event::BlockCommitted;
use hypercore_signer::Address as HypercoreAddress;
use parity_scale_codec::{Decode, Encode};
use std::sync::Arc;

#[derive(Debug, Encode, Decode)]
pub struct PendingUploadCode {
    pub origin: ActorId,
    pub code_id: CodeId,
    pub blob_tx: H256,
    pub tx_hash: H256,
}

impl PendingUploadCode {
    pub fn blob_tx(&self) -> H256 {
        if self.blob_tx.is_zero() {
            self.tx_hash
        } else {
            self.blob_tx
        }
    }
}

pub(crate) type ObserverProvider = RootProvider<BoxTransport>;

pub struct Observer {
    provider: ObserverProvider,
    router_address: Address,
    blob_reader: Arc<dyn BlobReader>,
}

impl Observer {
    const ROUTER_EVENT_SIGNATURE_HASHES: [B256; 11] = [
        B256::new(UploadCode::SIGNATURE_HASH),
        B256::new(CodeApproved::SIGNATURE_HASH),
        B256::new(CodeRejected::SIGNATURE_HASH),
        B256::new(CreateProgram::SIGNATURE_HASH),
        B256::new(UpdatedProgram::SIGNATURE_HASH),
        B256::new(UserMessageSent::SIGNATURE_HASH),
        B256::new(UserReplySent::SIGNATURE_HASH),
        B256::new(SendMessage::SIGNATURE_HASH),
        B256::new(SendReply::SIGNATURE_HASH),
        B256::new(ClaimValue::SIGNATURE_HASH),
        B256::new(BlockCommitted::SIGNATURE_HASH),
    ];

    pub async fn new(
        ethereum_rpc: &str,
        router_address: HypercoreAddress,
        blob_reader: Arc<dyn BlobReader>,
    ) -> Result<Self> {
        Ok(Self {
            provider: ProviderBuilder::new().on_builtin(ethereum_rpc).await?,
            router_address: Address::new(router_address.0),
            blob_reader,
        })
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
                        match block {
                            Some(block) => {
                                let block_header = block.header;
                                let block_hash = block_header.hash.expect("failed to get block hash");
                                let parent_hash = block_header.parent_hash;
                                let block_number = block_header.number.expect("failed to get block number");
                                let timestamp = block_header.timestamp;
                                log::info!("📦 receive block {block_number}, hash {block_hash}, parent hash: {parent_hash}");

                                match read_block_events(H256(block_hash.0), &self.provider, self.router_address).await {
                                    Ok((pending_upload_codes, events)) => {
                                        for pending_upload_code in pending_upload_codes.iter() {
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

                                        let block_data = BlockEventData {
                                            block_hash: H256(block_hash.0),
                                            parent_hash: H256(parent_hash.0),
                                            block_number,
                                            block_timestamp: timestamp,
                                            events,
                                            upload_codes: pending_upload_codes,
                                        };

                                        yield Event::Block(block_data);
                                    }
                                    Err(err) => log::error!("failed to read events: {err}"),
                                }
                            },
                            None => break,
                        }
                    }
                    future = futures.next(), if !futures.is_empty() => {
                        match future {
                            Some(future) => {
                                match future {
                                    Ok((origin, code_id, code)) => {
                                        yield Event::UploadCode(UploadCodeData { origin, code_id, code });
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
    router_address: Address,
) -> Result<(Vec<PendingUploadCode>, Vec<BlockEvent>)> {
    let router_events_filter = Filter::new()
        .at_block_hash(block_hash.0)
        .address(router_address)
        .event_signature(Topic::from_iter(Observer::ROUTER_EVENT_SIGNATURE_HASHES));

    let logs = provider.get_logs(&router_events_filter).await?;

    let mut pending_upload_codes = vec![];
    let block_events: Vec<_> = logs
        .into_iter()
        .filter_map(|ref log| match log.topic0().copied().map(|bytes| bytes.0) {
            Some(UploadCode::SIGNATURE_HASH) => {
                let UploadCode {
                    origin,
                    code_id,
                    blob_tx,
                } = log.try_into().ok()?;

                let tx_hash = H256(log.transaction_hash?.0);

                pending_upload_codes.push(PendingUploadCode {
                    origin,
                    code_id,
                    blob_tx,
                    tx_hash,
                });

                None
            }
            Some(CodeApproved::SIGNATURE_HASH) => {
                Some(BlockEvent::CodeApproved(log.try_into().ok()?))
            }
            Some(CodeRejected::SIGNATURE_HASH) => {
                Some(BlockEvent::CodeRejected(log.try_into().ok()?))
            }
            Some(CreateProgram::SIGNATURE_HASH) => {
                Some(BlockEvent::CreateProgram(log.try_into().ok()?))
            }
            Some(UpdatedProgram::SIGNATURE_HASH) => {
                Some(BlockEvent::UpdatedProgram(log.try_into().ok()?))
            }
            Some(UserMessageSent::SIGNATURE_HASH) => {
                Some(BlockEvent::UserMessageSent(log.try_into().ok()?))
            }
            Some(UserReplySent::SIGNATURE_HASH) => {
                Some(BlockEvent::UserReplySent(log.try_into().ok()?))
            }
            Some(SendMessage::SIGNATURE_HASH) => {
                Some(BlockEvent::SendMessage(log.try_into().ok()?))
            }
            Some(SendReply::SIGNATURE_HASH) => Some(BlockEvent::SendReply(log.try_into().ok()?)),
            Some(ClaimValue::SIGNATURE_HASH) => Some(BlockEvent::ClaimValue(log.try_into().ok()?)),
            Some(BlockCommitted::SIGNATURE_HASH) => {
                Some(BlockEvent::BlockCommitted(log.try_into().ok()?))
            }
            Some(hash) => {
                log::warn!("unexpected event signature hash: {}", H256(hash));
                None
            }
            None => None,
        })
        .collect();

    log::trace!(
        r#"read events for {block_hash}
        Upload codes: {pending_upload_codes:?}
        Block events: {block_events:?}"#
    );

    Ok((pending_upload_codes, block_events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockBlobReader;
    use alloy::node_bindings::Anvil;
    use hypercore_ethereum::Ethereum;
    use hypercore_signer::Signer;
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

    #[tokio::test]
    async fn test_deployment() -> Result<()> {
        let anvil = Anvil::new().try_spawn()?;
        let ethereum_rpc = anvil.ws_endpoint();

        let signer = Signer::new("/tmp/keys".into())?;

        let sender_public_key = signer.add_key(
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".parse()?,
        )?;
        let sender_address = sender_public_key.to_address();
        let validators = vec!["0x45D6536E3D4AdC8f4e13c5c4aA54bE968C55Abf1".parse()?];

        let ethereum = Ethereum::deploy(&ethereum_rpc, validators, signer, sender_address).await?;
        let blob_reader = Arc::new(MockBlobReader::default());

        let router_address = ethereum.router().address();
        let cloned_blob_reader = blob_reader.clone();

        let handle = task::spawn(async move {
            let mut observer = Observer::new(&ethereum_rpc, router_address, cloned_blob_reader)
                .await
                .expect("failed to create observer");

            let observer_events = observer.events();
            futures::pin_mut!(observer_events);

            while let Some(event) = observer_events.next().await {
                if matches!(event, Event::UploadCode { .. }) {
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

        let (tx_hash, _) = ethereum.router().upload_code_with_sidecar(&wasm).await?;
        blob_reader.add_blob_transaction(tx_hash, wasm).await;

        assert!(
            handle.await?.is_some(),
            "observer did not receive upload code event"
        );

        Ok(())
    }
}

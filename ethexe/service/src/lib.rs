// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::config::{Config, ConfigPublicKey};
use anyhow::{bail, Context, Result};
use ethexe_common::ProducerBlock;
use ethexe_compute::{BlockProcessed, ComputeEvent, ComputeService};
use ethexe_consensus::{
    BatchCommitmentValidationReply, BatchCommitmentValidationRequest, ConsensusEvent,
    ConsensusService, SimpleConnectService, ValidatorConfig, ValidatorService,
};
use ethexe_db::{Database, RocksDatabase};
use ethexe_ethereum::router::RouterQuery;
use ethexe_network::{NetworkEvent, NetworkService};
use ethexe_observer::{
    BlobData, BlobReader, ConsensusLayerBlobReader, MockBlobReader, ObserverEvent, ObserverService,
};
use ethexe_processor::{Processor, ProcessorConfig};
use ethexe_prometheus::{PrometheusEvent, PrometheusService};
use ethexe_rpc::{RpcEvent, RpcService};
use ethexe_service_utils::{OptionFuture as _, OptionStreamNext as _};
use ethexe_signer::{PublicKey, SignedData, Signer};
use ethexe_tx_pool::{SignedOffchainTransaction, TxPoolService};
use futures::StreamExt;
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::pin::Pin;
use tokio::sync::broadcast::Sender;

pub mod config;

mod fast_sync;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, derive_more::From)]
pub enum Event {
    // Fast sync done. Sent just once.
    FastSyncDone(H256),
    // Basic event to notify that service has started. Sent just once.
    ServiceStarted,
    // Services events.
    Compute(ComputeEvent),
    Consensus(ConsensusEvent),
    Network(NetworkEvent),
    Observer(ObserverEvent),
    Prometheus(PrometheusEvent),
    Rpc(RpcEvent),
}

/// ethexe service.
pub struct Service {
    db: Database,
    observer: ObserverService,
    compute: ComputeService,
    consensus: Pin<Box<dyn ConsensusService>>,
    signer: Signer,
    tx_pool: TxPoolService,

    // Optional services
    network: Option<NetworkService>,
    prometheus: Option<PrometheusService>,
    rpc: Option<RpcService>,

    fast_sync: bool,

    // Optional global event broadcaster.
    sender: Option<Sender<Event>>,
}

// TODO #4176: consider to move this to another module
#[derive(Debug, Clone, Encode, Decode, derive_more::From)]
pub enum NetworkMessage {
    ProducerBlock(SignedData<ProducerBlock>),
    RequestBatchValidation(SignedData<BatchCommitmentValidationRequest>),
    ApproveBatch(BatchCommitmentValidationReply),
    OffchainTransaction {
        transaction: SignedOffchainTransaction,
    },
}

impl Service {
    pub async fn new(config: &Config) -> Result<Self> {
        let (blob_reader, mock_blob_reader_for_rpc): (Box<dyn BlobReader>, Option<MockBlobReader>) =
            if config.node.dev {
                let reader = MockBlobReader::new();
                (Box::new(reader.clone()), Some(reader))
            } else {
                let reader = ConsensusLayerBlobReader::new(
                    &config.ethereum.rpc,
                    &config.ethereum.beacon_rpc,
                    config.ethereum.block_time,
                )
                .await
                .context("failed to create consensus layer blob reader")?;
                (Box::new(reader), None)
            };

        let rocks_db = RocksDatabase::open(
            config
                .node
                .database_path_for(config.ethereum.router_address),
        )
        .with_context(|| "failed to open database")?;
        let db = Database::from_one(&rocks_db);

        let observer = ObserverService::new(
            &config.ethereum,
            config.node.eth_max_sync_depth,
            db.clone(),
            blob_reader,
        )
        .await
        .context("failed to create observer service")?;

        let router_query = RouterQuery::new(&config.ethereum.rpc, config.ethereum.router_address)
            .await
            .with_context(|| "failed to create router query")?;

        let genesis_block_hash = router_query
            .genesis_block_hash()
            .await
            .with_context(|| "failed to query genesis hash")?;

        if genesis_block_hash.is_zero() {
            log::error!(
                "👶 Genesis block hash wasn't found. Call router.lookupGenesisHash() first"
            );

            bail!("Failed to query valid genesis hash");
        } else {
            log::info!("👶 Genesis block hash: {genesis_block_hash:?}");
        }

        let validators = router_query
            .validators()
            .await
            .with_context(|| "failed to query validators")?;
        log::info!("👥 Current validators set: {validators:?}");

        let threshold = router_query
            .threshold()
            .await
            .with_context(|| "failed to query validators threshold")?;
        log::info!("🔒 Multisig threshold: {threshold} / {}", validators.len());

        let processor = Processor::with_config(
            ProcessorConfig {
                worker_threads_override: config.node.worker_threads_override,
                virtual_threads: config.node.virtual_threads,
            },
            db.clone(),
        )
        .with_context(|| "failed to create processor")?;

        if let Some(worker_threads) = processor.config().worker_threads_override {
            log::info!("🔧 Overriding amount of physical threads for runtime: {worker_threads}");
        }

        log::info!(
            "🔧 Amount of virtual threads for programs processing: {}",
            processor.config().virtual_threads
        );

        let signer = Signer::fs(config.node.key_path.clone());

        let validator_pub_key = Self::get_config_public_key(config.node.validator, &signer)
            .with_context(|| "failed to get validator private key")?;

        // TODO #4642: use validator session key
        let _validator_pub_key_session =
            Self::get_config_public_key(config.node.validator_session, &signer)
                .with_context(|| "failed to get validator session private key")?;

        let consensus: Pin<Box<dyn ConsensusService>> = if let Some(pub_key) = validator_pub_key {
            Box::pin(
                ValidatorService::new(
                    signer.clone(),
                    db.clone(),
                    ValidatorConfig {
                        ethereum_rpc: config.ethereum.rpc.clone(),
                        router_address: config.ethereum.router_address,
                        pub_key,
                        signatures_threshold: threshold,
                        slot_duration: config.ethereum.block_time,
                    },
                )
                .await?,
            )
        } else {
            Box::pin(SimpleConnectService::new())
        };

        let prometheus = if let Some(config) = config.prometheus.clone() {
            Some(PrometheusService::new(config)?)
        } else {
            None
        };

        let network = if let Some(net_config) = &config.network {
            Some(
                NetworkService::new(net_config.clone(), &signer, db.clone())
                    .with_context(|| "failed to create network service")?,
            )
        } else {
            None
        };

        let rpc = config
            .rpc
            .as_ref()
            .map(|config| RpcService::new(config.clone(), db.clone(), mock_blob_reader_for_rpc));

        let tx_pool = TxPoolService::new(db.clone());

        let compute = ComputeService::new(db.clone(), processor);

        let fast_sync = config.node.fast_sync;

        Ok(Self {
            db,
            network,
            observer,
            compute,
            consensus,
            signer,
            prometheus,
            rpc,
            tx_pool,
            sender: None,
            fast_sync,
        })
    }

    fn get_config_public_key(key: ConfigPublicKey, signer: &Signer) -> Result<Option<PublicKey>> {
        match key {
            ConfigPublicKey::Enabled(key) => Ok(Some(key)),
            ConfigPublicKey::Random => Ok(Some(signer.generate_key()?)),
            ConfigPublicKey::Disabled => Ok(None),
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_from_parts(
        db: Database,
        observer: ObserverService,
        processor: Processor,
        signer: Signer,
        tx_pool: TxPoolService,
        consensus: Pin<Box<dyn ConsensusService>>,
        network: Option<NetworkService>,
        prometheus: Option<PrometheusService>,
        rpc: Option<RpcService>,
        sender: Option<Sender<Event>>,
        fast_sync: bool,
    ) -> Self {
        let compute = ComputeService::new(db.clone(), processor);

        Self {
            db,
            observer,
            compute,
            consensus,
            signer,
            network,
            prometheus,
            rpc,
            tx_pool,
            sender,
            fast_sync,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        if self.fast_sync {
            fast_sync::sync(&mut self).await?;
        }

        self.run_inner().await.inspect_err(|err| {
            log::error!("Service finished work with error: {err:?}");
        })
    }

    async fn run_inner(self) -> Result<()> {
        let Service {
            db,
            mut network,
            mut observer,
            mut compute,
            mut consensus,
            signer: _signer,
            tx_pool,
            mut prometheus,
            rpc,
            sender,
            fast_sync: _,
        } = self;

        let (mut rpc_handle, mut rpc) = if let Some(rpc) = rpc {
            log::info!("🌐 Rpc server starting at: {}", rpc.port());

            let (rpc_run, rpc_receiver) = rpc.run_server().await?;

            (Some(tokio::spawn(rpc_run.stopped())), Some(rpc_receiver))
        } else {
            (None, None)
        };

        let roles = vec!["Observer".to_string(), consensus.role()];
        log::info!("⚙️ Node service starting, roles: {roles:?}");

        // Broadcast service started event.
        // Never supposed to be Some in production code.
        if let Some(sender) = sender.as_ref() {
            sender
                .send(Event::ServiceStarted)
                .expect("failed to broadcast service STARTED event");
        }

        loop {
            let event: Event = tokio::select! {
                event = compute.select_next_some() => event?.into(),
                event = consensus.select_next_some() => event?.into(),
                event = network.maybe_next_some() => event.into(),
                event = observer.select_next_some() => event?.into(),
                event = prometheus.maybe_next_some() => event.into(),
                event = rpc.maybe_next_some() => event.into(),
                _ = rpc_handle.as_mut().maybe() => {
                    log::info!("`RPCWorker` has terminated, shutting down...");
                    continue;
                }
            };

            log::trace!("Primary service produced event, start handling: {event:?}");

            // Broadcast event.
            // Never supposed to be Some in production.
            if let Some(sender) = sender.as_ref() {
                sender
                    .send(event.clone())
                    .expect("failed to broadcast service event");
            }

            match event {
                Event::FastSyncDone(_) | Event::ServiceStarted => {
                    unreachable!("never handled here")
                }
                Event::Observer(event) => match event {
                    ObserverEvent::Blob(BlobData {
                        code_id,
                        timestamp,
                        code,
                    }) => {
                        log::info!(
                            "🔢 receive a code blob, code_id {code_id}, code size {}",
                            code.len()
                        );

                        compute.receive_code(code_id, timestamp, code)
                    }
                    ObserverEvent::Block(block_data) => {
                        log::info!(
                            "📦 receive a chain head, height {}, hash {}, parent hash {}",
                            block_data.header.height,
                            block_data.hash,
                            block_data.header.parent_hash,
                        );

                        consensus.receive_new_chain_head(block_data)?
                    }
                    ObserverEvent::BlockSynced(data) => {
                        // NOTE: Observer guarantees that, if this event is emitted,
                        // then from latest synced block and up to `block_hash`:
                        // 1) all blocks on-chain data (see OnChainStorage) is loaded and available in database.
                        // 2) all approved(at least) codes are loaded and available in database.

                        consensus.receive_synced_block(data)?
                    }
                },
                Event::Compute(event) => match event {
                    ComputeEvent::BlockProcessed(BlockProcessed { block_hash }) => {
                        consensus.receive_computed_block(block_hash)?
                    }
                    ComputeEvent::CodeProcessed(_) => {
                        // Nothing
                    }
                },
                Event::Network(event) => {
                    let Some(_) = network.as_mut() else {
                        unreachable!("couldn't produce event without network");
                    };

                    match event {
                        NetworkEvent::Message { source: _, data } => {
                            let Ok(message) = NetworkMessage::decode(&mut data.as_slice())
                                .inspect_err(|e| {
                                    log::warn!("Failed to decode network message: {e}")
                                })
                            else {
                                // TODO: use peer scoring for this case
                                continue;
                            };

                            match message {
                                NetworkMessage::ProducerBlock(block) => {
                                    consensus.receive_block_from_producer(block)?
                                }
                                NetworkMessage::RequestBatchValidation(request) => {
                                    consensus.receive_validation_request(request)?
                                }
                                NetworkMessage::ApproveBatch(reply) => {
                                    consensus.receive_validation_reply(reply)?
                                }
                                NetworkMessage::OffchainTransaction { transaction } => {
                                    if let Err(e) = Self::process_offchain_transaction(
                                        transaction,
                                        &tx_pool,
                                        &db,
                                        network.as_mut(),
                                    ) {
                                        log::warn!("Failed to process offchain transaction received by p2p: {e}");
                                    }
                                }
                            };
                        }
                        NetworkEvent::DbResponse { .. }
                        | NetworkEvent::PeerBlocked(_)
                        | NetworkEvent::PeerConnected(_) => (),
                    }
                }
                Event::Prometheus(event) => {
                    let Some(p) = prometheus.as_mut() else {
                        unreachable!("couldn't produce event without prometheus");
                    };

                    match event {
                        PrometheusEvent::CollectMetrics => {
                            let status = observer.status();

                            p.update_observer_metrics(status.eth_best_height, status.pending_codes);

                            // TODO #4643: support metrics for consensus service
                        }
                    }
                }
                Event::Rpc(event) => {
                    log::info!("Received RPC event: {event:#?}");

                    match event {
                        RpcEvent::OffchainTransaction {
                            transaction,
                            response_sender,
                        } => {
                            let res = Self::process_offchain_transaction(
                                transaction,
                                &tx_pool,
                                &db,
                                network.as_mut(),
                            )
                            .context("Failed to process offchain transaction received from RPC");

                            let Some(response_sender) = response_sender else {
                                unreachable!("Response sender isn't set for the `RpcEvent::OffchainTransaction` event");
                            };
                            if let Err(e) = response_sender.send(res) {
                                // No panic case as a responsibility of the service is fulfilled.
                                // The dropped receiver signalizes that the rpc service has crashed
                                // or is malformed, so problems should be handled there.
                                log::error!("Response receiver for the `RpcEvent::OffchainTransaction` was dropped: {e:#?}");
                            }
                        }
                    }
                }
                Event::Consensus(event) => match event {
                    ConsensusEvent::ComputeBlock(block) => compute.receive_synced_head(block),
                    ConsensusEvent::ComputeProducerBlock(producer_block) => {
                        if !producer_block.off_chain_transactions.is_empty()
                            || producer_block.gas_allowance.is_some()
                        {
                            todo!("#4638 #4639 off-chain transactions and gas allowance are not supported yet");
                        }

                        compute.receive_synced_head(producer_block.block_hash);
                    }
                    ConsensusEvent::PublishProducerBlock(block) => {
                        let Some(n) = network.as_mut() else {
                            continue;
                        };

                        n.publish_message(NetworkMessage::from(block).encode());
                    }
                    ConsensusEvent::PublishValidationRequest(request) => {
                        let Some(n) = network.as_mut() else {
                            continue;
                        };

                        n.publish_message(NetworkMessage::from(request).encode());
                    }
                    ConsensusEvent::PublishValidationReply(reply) => {
                        let Some(n) = network.as_mut() else {
                            continue;
                        };

                        n.publish_message(NetworkMessage::from(reply).encode());
                    }
                    ConsensusEvent::CommitmentSubmitted(tx) => {
                        log::info!("Commitment submitted, tx: {tx}");
                    }
                    ConsensusEvent::Warning(msg) => {
                        log::warn!("Consensus service warning: {msg}");
                    }
                },
            }
        }
    }

    fn process_offchain_transaction(
        transaction: SignedOffchainTransaction,
        tx_pool: &TxPoolService,
        db: &Database,
        network: Option<&mut NetworkService>,
    ) -> Result<H256> {
        let validated_tx = tx_pool
            .validate(transaction)
            .context("Failed to validate offchain transaction")?;
        let tx_hash = validated_tx.tx_hash();

        // Set valid transaction
        db.set_offchain_transaction(validated_tx.clone());

        // Try propagate transaction
        if let Some(n) = network {
            n.publish_offchain_transaction(
                NetworkMessage::OffchainTransaction {
                    transaction: validated_tx,
                }
                .encode(),
            );
        } else {
            log::debug!(
                "Validated offchain transaction won't be propagated, network service isn't defined"
            );
        }

        // TODO (breathx) Execute transaction
        log::info!("Unimplemented tx execution");

        Ok(tx_hash)
    }
}

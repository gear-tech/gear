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
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use ethexe_blob_loader::{
    BlobLoader, BlobLoaderEvent, BlobLoaderService, ConsensusLayerConfig,
    local::{LocalBlobLoader, LocalBlobStorage},
};
use ethexe_common::{ecdsa::PublicKey, gear::CodeState, network::NetworkMessage};
use ethexe_compute::{ComputeEvent, ComputeService};
use ethexe_consensus::{
    ConsensusEvent, ConsensusService, SimpleConnectService, ValidatorConfig, ValidatorService,
};
use ethexe_db::{Database, RocksDatabase};
use ethexe_ethereum::router::RouterQuery;
use ethexe_network::{NetworkEvent, NetworkService, db_sync::ExternalDataProvider};
use ethexe_observer::{ObserverEvent, ObserverService};
use ethexe_processor::{Processor, ProcessorConfig};
use ethexe_prometheus::{PrometheusEvent, PrometheusService};
use ethexe_rpc::{RpcEvent, RpcService};
use ethexe_service_utils::{OptionFuture as _, OptionStreamNext as _};
use ethexe_signer::Signer;
use ethexe_tx_pool::{TxPoolEvent, TxPoolService};
use futures::StreamExt;
use gprimitives::{ActorId, CodeId, H256};
use std::{collections::BTreeSet, pin::Pin};

pub mod config;

mod fast_sync;
#[cfg(test)]
mod tests;

#[derive(Debug, derive_more::From)]
pub enum Event {
    Compute(ComputeEvent),
    Consensus(ConsensusEvent),
    Network(NetworkEvent),
    Observer(ObserverEvent),
    BlobLoader(BlobLoaderEvent),
    Prometheus(PrometheusEvent),
    Rpc(RpcEvent),
    TxPool(TxPoolEvent),
}

#[derive(Clone)]
struct RouterDataProvider(RouterQuery);

#[async_trait]
impl ExternalDataProvider for RouterDataProvider {
    fn clone_boxed(&self) -> Box<dyn ExternalDataProvider> {
        Box::new(self.clone())
    }

    async fn programs_code_ids_at(
        self: Box<Self>,
        program_ids: BTreeSet<ActorId>,
        block: H256,
    ) -> Result<Vec<CodeId>> {
        self.0.programs_code_ids_at(program_ids, block).await
    }

    async fn codes_states_at(
        self: Box<Self>,
        code_ids: BTreeSet<CodeId>,
        block: H256,
    ) -> Result<Vec<CodeState>> {
        self.0.codes_states_at(code_ids, block).await
    }
}

/// ethexe service.
pub struct Service {
    db: Database,
    observer: ObserverService,
    blob_loader: Box<dyn BlobLoaderService>,
    compute: ComputeService,
    consensus: Pin<Box<dyn ConsensusService>>,
    signer: Signer,
    tx_pool: TxPoolService,

    // Optional services
    network: Option<NetworkService>,
    prometheus: Option<PrometheusService>,
    rpc: Option<RpcService>,

    fast_sync: bool,

    #[cfg(test)]
    sender: tests::utils::TestingEventSender,
}

impl Service {
    pub async fn new(config: &Config) -> Result<Self> {
        let rocks_db = RocksDatabase::open(
            config
                .node
                .database_path_for(config.ethereum.router_address),
        )
        .with_context(|| "failed to open database")?;
        let db = Database::from_one(&rocks_db);

        let (blob_loader, local_blob_storage_for_rpc) = if config.node.dev {
            let storage = LocalBlobStorage::default();
            let blob_loader = LocalBlobLoader::new(storage.clone());

            (blob_loader.into_box(), Some(storage))
        } else {
            let consensus_config = ConsensusLayerConfig {
                ethereum_rpc: config.ethereum.rpc.clone(),
                ethereum_beacon_rpc: config.ethereum.beacon_rpc.clone(),
                beacon_block_time: config.ethereum.block_time,
            };
            let blob_loader = BlobLoader::new(db.clone(), consensus_config)
                .await
                .context("failed to create blob loader")?;

            (blob_loader.into_box(), None)
        };

        let observer =
            ObserverService::new(&config.ethereum, config.node.eth_max_sync_depth, db.clone())
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
                chunk_processing_threads: config.node.chunk_processing_threads,
            },
            db.clone(),
        )
        .with_context(|| "failed to create processor")?;

        log::info!(
            "🔧 Amount of chunk processing threads for programs processing: {}",
            processor.config().chunk_processing_threads
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
                        fallbacks_rpc: config.ethereum.fallback_rpc.clone(),
                        router_address: config.ethereum.router_address,
                        pub_key,
                        signatures_threshold: threshold,
                        slot_duration: config.ethereum.block_time,
                        block_gas_limit: config.node.block_gas_limit,
                    },
                )
                .await?,
            )
        } else {
            Box::pin(SimpleConnectService::new(
                db.clone(),
                config.ethereum.block_time,
            ))
        };

        let prometheus = if let Some(config) = config.prometheus.clone() {
            Some(PrometheusService::new(config)?)
        } else {
            None
        };

        let network = if let Some(net_config) = &config.network {
            Some(
                NetworkService::new(
                    net_config.clone(),
                    &signer,
                    Box::new(RouterDataProvider(router_query)),
                    Box::new(db.clone()),
                )
                .with_context(|| "failed to create network service")?,
            )
        } else {
            None
        };

        let rpc = config
            .rpc
            .as_ref()
            .map(|config| RpcService::new(config.clone(), db.clone(), local_blob_storage_for_rpc));

        let tx_pool = TxPoolService::new(db.clone());

        let compute = ComputeService::new(db.clone(), processor);

        let fast_sync = config.node.fast_sync;

        #[allow(unreachable_code)]
        Ok(Self {
            db,
            network,
            observer,
            blob_loader,
            compute,
            consensus,
            signer,
            prometheus,
            rpc,
            tx_pool,
            fast_sync,
            #[cfg(test)]
            sender: unreachable!(),
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
        blob_loader: Box<dyn BlobLoaderService>,
        processor: Processor,
        signer: Signer,
        tx_pool: TxPoolService,
        consensus: Pin<Box<dyn ConsensusService>>,
        network: Option<NetworkService>,
        prometheus: Option<PrometheusService>,
        rpc: Option<RpcService>,
        sender: tests::utils::TestingEventSender,
        fast_sync: bool,
    ) -> Self {
        let compute = ComputeService::new(db.clone(), processor);

        Self {
            db,
            observer,
            blob_loader,
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
            db: _,
            mut network,
            mut observer,
            mut blob_loader,
            mut compute,
            mut consensus,
            signer: _signer,
            mut tx_pool,
            mut prometheus,
            rpc,
            fast_sync: _,
            #[cfg(test)]
            sender,
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

        #[cfg(test)]
        sender
            .send(tests::utils::TestingEvent::ServiceStarted)
            .expect("failed to broadcast service STARTED event");

        loop {
            let event: Event = tokio::select! {
                event = compute.select_next_some() => event?.into(),
                event = consensus.select_next_some() => event?.into(),
                event = network.maybe_next_some() => event.into(),
                event = observer.select_next_some() => event?.into(),
                event = blob_loader.select_next_some() => event?.into(),
                event = prometheus.maybe_next_some() => event.into(),
                event = rpc.maybe_next_some() => event.into(),
                event = tx_pool.select_next_some() => event.into(),
                _ = rpc_handle.as_mut().maybe() => {
                    log::info!("`RPCWorker` has terminated, shutting down...");
                    continue;
                }
            };

            log::trace!("Primary service produced event, start handling: {event:?}");

            #[cfg(test)]
            sender
                .send(tests::utils::TestingEvent::new(&event))
                .expect("failed to broadcast service event");

            match event {
                Event::Observer(event) => match event {
                    ObserverEvent::Block(block_data) => {
                        log::info!(
                            "📦 receive a chain head, height {}, hash {}, parent hash {}",
                            block_data.header.height,
                            block_data.hash,
                            block_data.header.parent_hash,
                        );

                        consensus.receive_new_chain_head(block_data)?
                    }
                    ObserverEvent::BlockSynced(block_hash) => {
                        // NOTE: Observer guarantees that, if `BlockSynced` event is emitted,
                        // then from latest synced block and up to `data.block_hash`:
                        // all blocks on-chain data (see OnChainStorage) is loaded and available in database.

                        compute.prepare_block(block_hash);
                        consensus.receive_synced_block(block_hash)?;
                    }
                },
                Event::BlobLoader(event) => match event {
                    BlobLoaderEvent::BlobLoaded(code_and_id) => {
                        compute.process_code(code_and_id);
                    }
                },
                Event::Compute(event) => match event {
                    ComputeEvent::RequestLoadCodes(codes) => {
                        blob_loader.load_codes(codes, None)?;
                    }
                    ComputeEvent::AnnounceComputed(announce_hash) => {
                        consensus.receive_computed_announce(announce_hash)?
                    }
                    ComputeEvent::AnnounceRejected(announce_hash) => {
                        // TODO: #4811 we should handle this case properly inside consensus service
                        log::warn!("Announce {announce_hash:?} was rejected");
                    }
                    ComputeEvent::BlockPrepared(block_hash) => {
                        consensus.receive_prepared_block(block_hash)?
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
                        NetworkEvent::Message(message) => {
                            match message {
                                NetworkMessage::ProducerBlock(block) => {
                                    consensus.receive_announce(block)?
                                }
                                NetworkMessage::RequestBatchValidation(request) => {
                                    consensus.receive_validation_request(request)?
                                }
                                NetworkMessage::ApproveBatch(reply) => {
                                    consensus.receive_validation_reply(reply)?
                                }
                            };
                        }
                        NetworkEvent::OffchainTransaction(transaction) => {
                            if let Err(e) = tx_pool.process_offchain_transaction(transaction) {
                                log::warn!(
                                    "Failed to process offchain transaction received by p2p: {e}"
                                );
                            }
                        }
                        NetworkEvent::PeerBlocked(_) | NetworkEvent::PeerConnected(_) => {}
                    }
                }
                Event::Prometheus(event) => {
                    let Some(p) = prometheus.as_mut() else {
                        unreachable!("couldn't produce event without prometheus");
                    };

                    match event {
                        PrometheusEvent::CollectMetrics => {
                            let last_block = observer.last_block_number();
                            let pending_codes = blob_loader.pending_codes_len();

                            p.update_observer_metrics(last_block, pending_codes);

                            // Collect compute service metrics
                            let metrics = compute.get_metrics();

                            p.update_compute_metrics(
                                metrics.blocks_queue_len,
                                metrics.waiting_codes_count,
                                metrics.process_codes_count,
                            );

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
                            let res = tx_pool.process_offchain_transaction(transaction).context(
                                "Failed to process offchain transaction received from RPC",
                            );

                            let Some(response_sender) = response_sender else {
                                unreachable!(
                                    "Response sender isn't set for the `RpcEvent::OffchainTransaction` event"
                                );
                            };
                            if let Err(e) = response_sender.send(res) {
                                // No panic case as a responsibility of the service is fulfilled.
                                // The dropped receiver signalizes that the rpc service has crashed
                                // or is malformed, so problems should be handled there.
                                log::error!(
                                    "Response receiver for the `RpcEvent::OffchainTransaction` was dropped: {e:#?}"
                                );
                            }
                        }
                    }
                }
                Event::Consensus(event) => match event {
                    ConsensusEvent::ComputeAnnounce(announce) => compute.compute_announce(announce),
                    ConsensusEvent::PublishAnnounce(block) => {
                        let Some(n) = network.as_mut() else {
                            continue;
                        };

                        n.publish_message(block);
                    }
                    ConsensusEvent::PublishValidationRequest(request) => {
                        let Some(n) = network.as_mut() else {
                            continue;
                        };

                        n.publish_message(request);
                    }
                    ConsensusEvent::PublishValidationReply(reply) => {
                        let Some(n) = network.as_mut() else {
                            continue;
                        };

                        n.publish_message(reply);
                    }
                    ConsensusEvent::CommitmentSubmitted(tx) => {
                        log::info!("Commitment submitted, tx: {tx}");
                    }
                    ConsensusEvent::Warning(msg) => {
                        log::warn!("Consensus service warning: {msg}");
                    }
                },
                Event::TxPool(event) => match event {
                    TxPoolEvent::PublishOffchainTransaction(transaction) => {
                        let Some(n) = network.as_mut() else {
                            log::debug!(
                                "Validated offchain transaction won't be propagated, network service isn't defined"
                            );

                            continue;
                        };

                        n.publish_offchain_transaction(transaction);
                    }
                },
            }
        }
    }
}

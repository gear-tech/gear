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
use alloy::primitives::U256;
use anyhow::{bail, Context, Result};
use ethexe_common::ProducerBlock;
use ethexe_compute::{BlockProcessed, ComputeEvent, ComputeService};
use ethexe_control::{
    BatchCommitmentValidationReply, BatchCommitmentValidationRequest, ControlEvent, ControlService,
    SimpleConnectService, ValidatorConfig, ValidatorService,
};
use ethexe_db::{Database, RocksDatabase};
use ethexe_ethereum::router::RouterQuery;
use ethexe_network::{db_sync, NetworkEvent, NetworkService};
use ethexe_observer::{BlobData, MockBlobReader, ObserverEvent, ObserverService};
use ethexe_processor::{Processor, ProcessorConfig};
use ethexe_prometheus::{PrometheusEvent, PrometheusService};
use ethexe_rpc::{RpcEvent, RpcService};
use ethexe_service_utils::{OptionFuture as _, OptionStreamNext as _};
use ethexe_signer::{PublicKey, SignedData, Signer};
use ethexe_tx_pool::{SignedOffchainTransaction, TxPoolService};
use futures::StreamExt;
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::{pin::Pin, sync::Arc};
use tokio::sync::broadcast::Sender;

pub mod config;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, derive_more::From)]
pub enum Event {
    // Basic event to notify that service has started. Sent just once.
    ServiceStarted,
    // Services events.
    Compute(ComputeEvent),
    Control(ControlEvent),
    Network(NetworkEvent),
    Observer(ObserverEvent),
    Prometheus(PrometheusEvent),
    Rpc(RpcEvent),
}

/// ethexe service.
pub struct Service {
    db: Database,
    observer: ObserverService,
    router_query: RouterQuery,
    compute: ComputeService,
    control: Pin<Box<dyn ControlService>>,
    signer: Signer,
    tx_pool: TxPoolService,

    // Optional services
    network: Option<NetworkService>,
    prometheus: Option<PrometheusService>,
    rpc: Option<RpcService>,

    // Optional global event broadcaster.
    sender: Option<Sender<Event>>,
}

// TODO #4176: consider to move this to another module
#[derive(Debug, Clone, Encode, Decode, derive_more::From)]
pub enum NetworkMessage {
    #[from]
    ProducerBlock(SignedData<ProducerBlock>),
    #[from]
    RequestBatchValidation(SignedData<BatchCommitmentValidationRequest>),
    #[from]
    ApproveBatch(BatchCommitmentValidationReply),
    #[from]
    OffchainTransaction {
        transaction: SignedOffchainTransaction,
    },
}

impl Service {
    pub async fn new(config: &Config) -> Result<Self> {
        let mock_blob_reader: Option<Arc<MockBlobReader>> = if config.node.dev {
            Some(Arc::new(MockBlobReader::new()))
        } else {
            None
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
            mock_blob_reader.clone().map(|r| r as _),
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

        let signer =
            Signer::new(config.node.key_path.clone()).with_context(|| "failed to create signer")?;

        let validator_pub_key = Self::get_config_public_key(config.node.validator, &signer)
            .with_context(|| "failed to get validator private key")?;

        // TODO +_+_+: use validator session key
        let _validator_pub_key_session =
            Self::get_config_public_key(config.node.validator_session, &signer)
                .with_context(|| "failed to get validator session private key")?;

        let control: Pin<Box<dyn ControlService>> = if let Some(pub_key) = validator_pub_key {
            Box::pin(
                ValidatorService::new(
                    signer.clone(),
                    db.clone(),
                    ValidatorConfig {
                        ethereum_rpc: config.ethereum.rpc.clone(),
                        router_address: config.ethereum.router_address,
                        pub_key,
                        threshold,
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
            .map(|config| RpcService::new(config.clone(), db.clone(), mock_blob_reader.clone()));

        let tx_pool = TxPoolService::new(db.clone());

        let compute = ComputeService::new(db.clone(), processor);

        Ok(Self {
            db,
            network,
            observer,
            compute,
            control,
            router_query,
            signer,
            prometheus,
            rpc,
            tx_pool,
            sender: None,
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
        router_query: RouterQuery,
        processor: Processor,
        signer: Signer,
        tx_pool: TxPoolService,
        control: Pin<Box<dyn ControlService>>,
        network: Option<NetworkService>,
        prometheus: Option<PrometheusService>,
        rpc: Option<RpcService>,
        sender: Option<Sender<Event>>,
    ) -> Self {
        let compute = ComputeService::new(db.clone(), processor);

        Self {
            db,
            observer,
            compute,
            control,
            router_query,
            signer,
            network,
            prometheus,
            rpc,
            tx_pool,
            sender,
        }
    }

    pub async fn run(self) -> Result<()> {
        self.run_inner().await.inspect_err(|err| {
            log::error!("Service finished work with error: {err:?}");
        })
    }

    async fn run_inner(self) -> Result<()> {
        let Service {
            db,
            mut network,
            mut observer,
            mut router_query,
            mut compute,
            mut control,
            signer: _signer,
            tx_pool,
            mut prometheus,
            rpc,
            sender,
        } = self;

        let (mut rpc_handle, mut rpc) = if let Some(rpc) = rpc {
            log::info!("🌐 Rpc server starting at: {}", rpc.port());

            let (rpc_run, rpc_receiver) = rpc.run_server().await?;

            (Some(tokio::spawn(rpc_run.stopped())), Some(rpc_receiver))
        } else {
            (None, None)
        };

        let mut roles = "Observer".to_string();
        roles.push_str(control.role().as_str());

        log::info!("⚙️ Node service starting, roles: [{}]", roles);

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
                event = control.select_next_some() => event?.into(),
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
                Event::ServiceStarted => unreachable!("never handled here"),
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

                        control.receive_new_chain_head(block_data)?
                    }
                    ObserverEvent::BlockSynced(data) => {
                        // NOTE: Observer guarantees that, if this event is emitted,
                        // then from latest synced block and up to `block_hash`:
                        // 1) all blocks on-chain data (see OnChainStorage) is loaded and available in database.
                        // 2) all approved(at least) codes are loaded and available in database.

                        control.receive_synced_block(data)?
                    }
                },
                Event::Compute(event) => match event {
                    ComputeEvent::BlockProcessed(BlockProcessed { block_hash }) => {
                        control.receive_computed_block(block_hash)?
                    }
                    ComputeEvent::CodeProcessed(_) => {
                        // Nothing
                    }
                },
                Event::Network(event) => {
                    let Some(n) = network.as_mut() else {
                        unreachable!("couldn't produce event without network");
                    };

                    match event {
                        NetworkEvent::Message { source, data } => {
                            log::trace!("Received a network message from peer {source:?}");

                            let Ok(message) = NetworkMessage::decode(&mut data.as_slice())
                                .inspect_err(|e| {
                                    log::warn!("Failed to decode network message: {e}")
                                })
                            else {
                                continue;
                            };

                            match message {
                                NetworkMessage::ProducerBlock(block) => {
                                    control.receive_block_from_producer(block)?
                                }
                                NetworkMessage::RequestBatchValidation(request) => {
                                    control.receive_validation_request(request)?
                                }
                                NetworkMessage::ApproveBatch(reply) => {
                                    control.receive_validation_reply(reply)?
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
                        NetworkEvent::ExternalValidation(validating_response) => {
                            let validated = Self::process_response_validation(
                                &validating_response,
                                &mut router_query,
                            )
                            .await?;

                            let res = if validated {
                                Ok(validating_response)
                            } else {
                                Err(validating_response)
                            };

                            n.request_validated(res);
                        }
                        NetworkEvent::DbResponse(_)
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

                            // TODO +_+_+: support metrics for control service
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
                Event::Control(event) => match event {
                    ControlEvent::ComputeBlock(block) => compute.receive_synced_head(block),
                    ControlEvent::ComputeProducerBlock(producer_block) => {
                        if !producer_block.off_chain_transactions.is_empty()
                            || producer_block.gas_allowance.is_none()
                        {
                            todo!("+_+_+ off-chain transactions and gas allowance are not supported yet");
                        }

                        compute.receive_synced_head(producer_block.block_hash);
                    }
                    ControlEvent::PublishProducerBlock(block) => {
                        let Some(n) = network.as_mut() else {
                            continue;
                        };

                        n.publish_message(NetworkMessage::from(block).encode());
                    }
                    ControlEvent::PublishValidationRequest(request) => {
                        let Some(n) = network.as_mut() else {
                            continue;
                        };

                        n.publish_message(NetworkMessage::from(request).encode());
                    }
                    ControlEvent::PublishValidationReply(reply) => {
                        let Some(n) = network.as_mut() else {
                            continue;
                        };

                        n.publish_message(NetworkMessage::from(reply).encode());
                    }
                    ControlEvent::CommitmentSubmitted(tx) => {
                        log::info!("Commitment submitted, tx: {tx}");
                    }
                    ControlEvent::Warning(msg) => {
                        log::warn!("Control service warning: {msg}");
                    }
                },
            }
        }
    }

    async fn process_response_validation(
        validating_response: &db_sync::ValidatingResponse,
        router_query: &mut RouterQuery,
    ) -> Result<bool> {
        let response = validating_response.response();

        if let db_sync::Response::ProgramIds(ids) = response {
            let ethereum_programs = router_query.programs_count().await?;
            if ethereum_programs != U256::from(ids.len()) {
                return Ok(false);
            }

            // TODO: #4309
            for &id in ids {
                let code_id = router_query.program_code_id(id).await?;
                if code_id.is_none() {
                    return Ok(false);
                }
            }
        }

        Ok(true)
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

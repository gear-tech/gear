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
use anyhow::{anyhow, bail, Context, Result};
use ethexe_common::gear::{BlockCommitment, CodeCommitment};
use ethexe_compute::{BlockProcessed, ComputeEvent, ComputeService};
use ethexe_db::Database;
use ethexe_ethereum::router::RouterQuery;
use ethexe_network::{db_sync, NetworkEvent, NetworkService};
use ethexe_observer::{MockBlobReader, ObserverEvent, ObserverService};
use ethexe_processor::ProcessorConfig;
use ethexe_prometheus::{PrometheusEvent, PrometheusService};
use ethexe_rpc::RpcEvent;
use ethexe_sequencer::{
    agro::AggregatedCommitments, SequencerConfig, SequencerEvent, SequencerService,
};
use ethexe_service_utils::{OptionFuture as _, OptionStreamNext as _};
use ethexe_signer::{Digest, PublicKey, Signature, Signer};
use ethexe_validator::BlockCommitmentValidationRequest;
use futures::StreamExt;
use parity_scale_codec::{Decode, Encode};
use std::sync::Arc;
use tokio::sync::broadcast::Sender;

pub mod config;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, derive_more::From)]
pub enum ServiceEvent {
    // Basic event to notify that service has started. Sent just once.
    ServiceStarted,
    // Services events.
    Compute(ComputeEvent),
    Network(NetworkEvent),
    Observer(ObserverEvent),
    Prometheus(PrometheusEvent),
    Rpc(RpcEvent),
    Sequencer(SequencerEvent),
}

/// ethexe service.
pub struct Service {
    db: Database,
    observer: ObserverService,
    router_query: RouterQuery,
    compute: ComputeService,
    signer: ethexe_signer::Signer,

    // Optional services
    network: Option<NetworkService>,
    sequencer: Option<SequencerService>,
    validator: Option<ethexe_validator::Validator>,
    prometheus: Option<PrometheusService>,
    rpc: Option<ethexe_rpc::RpcService>,

    // Optional global event broadcaster.
    sender: Option<Sender<ServiceEvent>>,
}

// TODO: consider to move this to another module #4176
#[derive(Debug, Clone, Encode, Decode)]
pub enum NetworkMessage {
    PublishCommitments {
        codes: Option<AggregatedCommitments<CodeCommitment>>,
        blocks: Option<AggregatedCommitments<BlockCommitment>>,
    },
    RequestCommitmentsValidation {
        codes: Vec<CodeCommitment>,
        blocks: Vec<BlockCommitmentValidationRequest>,
    },
    ApproveCommitments {
        batch_commitment: (Digest, Signature),
    },
}

impl Service {
    pub async fn new(config: &Config) -> Result<Self> {
        let mock_blob_reader: Option<Arc<MockBlobReader>> = if config.node.dev {
            Some(Arc::new(MockBlobReader::new(config.ethereum.block_time)))
        } else {
            None
        };

        let blob_reader: Arc<dyn ethexe_observer::BlobReader> = if config.node.dev {
            mock_blob_reader.clone().unwrap()
        } else {
            Arc::new(
                ethexe_observer::ConsensusLayerBlobReader::new(
                    &config.ethereum.rpc,
                    &config.ethereum.beacon_rpc,
                    config.ethereum.block_time,
                )
                .await
                .with_context(|| "failed to create blob reader")?,
            )
        };

        let rocks_db = ethexe_db::RocksDatabase::open(config.node.database_path.clone())
            .with_context(|| "failed to open database")?;
        let db = ethexe_db::Database::from_one(&rocks_db, config.ethereum.router_address.0);

        let observer = ObserverService::new(&config.ethereum)
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
        log::info!("👥 Validators set: {validators:?}");

        let threshold = router_query
            .threshold()
            .await
            .with_context(|| "failed to query validators threshold")?;
        log::info!("🔒 Multisig threshold: {threshold} / {}", validators.len());

        let query = ethexe_observer::Query::new(
            Arc::new(db.clone()),
            &config.ethereum.rpc,
            config.ethereum.router_address,
            genesis_block_hash,
            blob_reader,
            config.node.max_commitment_depth,
        )
        .await
        .with_context(|| "failed to create observer query")?;

        let processor = ethexe_processor::Processor::with_config(
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

        let signer = ethexe_signer::Signer::new(config.node.key_path.clone())
            .with_context(|| "failed to create signer")?;

        let sequencer = if let Some(key) =
            Self::get_config_public_key(config.node.sequencer, &signer)
                .with_context(|| "failed to get sequencer private key")?
        {
            Some(
                SequencerService::new(
                    &SequencerConfig {
                        ethereum_rpc: config.ethereum.rpc.clone(),
                        sign_tx_public: key,
                        router_address: config.ethereum.router_address,
                        validators,
                        threshold,
                        block_time: config.ethereum.block_time,
                    },
                    signer.clone(),
                    Box::new(db.clone()),
                )
                .await
                .with_context(|| "failed to create sequencer")?,
            )
        } else {
            None
        };

        let validator_pub_key = Self::get_config_public_key(config.node.validator, &signer)
            .with_context(|| "failed to get validator private key")?;
        let validator_pub_key_session =
            Self::get_config_public_key(config.node.validator_session, &signer)
                .with_context(|| "failed to get validator session private key")?;
        let validator =
            validator_pub_key
                .zip(validator_pub_key_session)
                .map(|(pub_key, pub_key_session)| {
                    ethexe_validator::Validator::new(
                        &ethexe_validator::Config {
                            pub_key,
                            pub_key_session,
                            router_address: config.ethereum.router_address,
                        },
                        signer.clone(),
                    )
                });

        let prometheus = if let Some(config) = config.prometheus.clone() {
            Some(PrometheusService::new(config)?)
        } else {
            None
        };

        let network = if let Some(net_config) = &config.network {
            Some(
                ethexe_network::NetworkService::new(net_config.clone(), &signer, db.clone())
                    .with_context(|| "failed to create network service")?,
            )
        } else {
            None
        };

        let rpc = config.rpc.as_ref().map(|config| {
            ethexe_rpc::RpcService::new(config.clone(), db.clone(), mock_blob_reader.clone())
        });

        let compute = ComputeService::new(db.clone(), processor, query);

        Ok(Self {
            db,
            network,
            observer,
            compute,
            router_query,
            sequencer,
            signer,
            validator,
            prometheus,
            rpc,
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
        query: ethexe_observer::Query,
        router_query: RouterQuery,
        processor: ethexe_processor::Processor,
        signer: ethexe_signer::Signer,
        network: Option<NetworkService>,
        sequencer: Option<SequencerService>,
        validator: Option<ethexe_validator::Validator>,
        prometheus: Option<PrometheusService>,
        rpc: Option<ethexe_rpc::RpcService>,
        sender: Option<Sender<ServiceEvent>>,
    ) -> Self {
        let compute = ComputeService::new(db.clone(), processor.clone(), query.clone());

        Self {
            db,
            observer,
            compute,
            router_query,
            signer,
            network,
            sequencer,
            validator,
            prometheus,
            rpc,
            sender,
        }
    }

    pub async fn run(self) -> Result<()> {
        self.run_inner().await.map_err(|err| {
            log::error!("Service finished work with error: {err:?}");
            err
        })
    }

    async fn run_inner(self) -> Result<()> {
        let Service {
            db,
            mut network,
            mut observer,
            mut router_query,
            mut compute,
            mut sequencer,
            signer: _signer,
            mut validator,
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
        if let Some(seq) = sequencer.as_ref() {
            roles.push_str(&format!(", Sequencer ({})", seq.address()));
        }
        if let Some(val) = validator.as_ref() {
            roles.push_str(&format!(", Validator ({})", val.address()));
        }

        log::info!("⚙️ Node service starting, roles: [{}]", roles);

        // Broadcast service started event.
        if let Some(sender) = sender.as_ref() {
            sender
                .send(ServiceEvent::ServiceStarted)
                .map_err(|e| anyhow!("failed to broadcast service STARTED event: {e}"))?;
        }

        loop {
            let event: ServiceEvent = tokio::select! {
                event = compute.select_next_some() => event?.into(),
                event = network.maybe_next_some() => event.into(),
                event = observer.select_next_some() => event?.into(),
                event = prometheus.maybe_next_some() => event.into(),
                event = rpc.maybe_next_some() => event.into(),
                event = sequencer.maybe_next_some() => event.into(),
                _ = rpc_handle.as_mut().maybe() => {
                    log::info!("`RPCWorker` has terminated, shutting down...");
                    continue;
                }
            };

            log::trace!("Primary service produced event, start handling: {event:?}");

            // Broadcast event.
            if let Some(sender) = sender.as_ref() {
                sender
                    .send(event.clone())
                    .map_err(|e| anyhow!("failed to broadcast service event: {e}"))?;
            }

            match event {
                ServiceEvent::ServiceStarted => unreachable!("never handled here"),
                ServiceEvent::Compute(event) => match event {
                    ComputeEvent::BlockProcessed(BlockProcessed {
                        chain_head,
                        commitments,
                    }) => {
                        // TODO (gsobol): must be done in observer event handling
                        if let Some(s) = sequencer.as_mut() {
                            s.on_new_head(chain_head)?
                        }

                        if commitments.is_empty() {
                            continue;
                        }

                        if let Some(v) = validator.as_mut() {
                            let aggregated_block_commitments = v.aggregate(commitments)?;

                            if let Some(n) = network.as_mut() {
                                log::debug!("Publishing block commitments to network...");

                                n.publish_message(
                                    NetworkMessage::PublishCommitments {
                                        codes: None,
                                        blocks: Some(aggregated_block_commitments.clone()),
                                    }
                                    .encode(),
                                );
                            };

                            if let Some(s) = sequencer.as_mut() {
                                log::debug!(
                                    "Received ({}) signed block commitments from local validator...",
                                    aggregated_block_commitments.len()
                                );

                                s.receive_block_commitments(aggregated_block_commitments)?;
                            }
                        };
                    }
                    ComputeEvent::CodeProcessed(commitment) => {
                        if let Some(v) = validator.as_mut() {
                            let aggregated_code_commitments = v.aggregate(vec![commitment])?;

                            if let Some(n) = network.as_mut() {
                                log::debug!("Publishing code commitments to network...");
                                n.publish_message(
                                    NetworkMessage::PublishCommitments {
                                        codes: Some(aggregated_code_commitments.clone()),
                                        blocks: None,
                                    }
                                    .encode(),
                                );
                            };

                            if let Some(s) = sequencer.as_mut() {
                                log::debug!(
                                    "Received ({}) signed code commitments from local validator...",
                                    aggregated_code_commitments.len()
                                );
                                s.receive_code_commitments(aggregated_code_commitments)?;
                            }
                        }
                    }
                },
                ServiceEvent::Network(event) => {
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
                                NetworkMessage::PublishCommitments { codes, blocks } => {
                                    if let Some(s) = sequencer.as_mut() {
                                        if let Some(aggregated) = codes {
                                            let _ = s.receive_code_commitments(aggregated)
                                            .inspect_err(|e| log::warn!("failed to receive code commitments from network: {e}"));
                                        }

                                        if let Some(aggregated) = blocks {
                                            let _ = s.receive_block_commitments(aggregated)
                                            .inspect_err(|e| log::warn!("failed to receive block commitments from network: {e}"));
                                        }
                                    }
                                }
                                NetworkMessage::RequestCommitmentsValidation { codes, blocks } => {
                                    if let Some(v) = validator.as_mut() {
                                        let maybe_batch_commitment = (!codes.is_empty() || !blocks.is_empty())
                                        .then(|| v.validate_batch_commitment(&db, codes, blocks))
                                        .transpose()
                                        .inspect_err(|e| log::warn!("failed to validate batch commitment from network: {e}"))
                                        .ok()
                                        .flatten();

                                        if let Some(batch_commitment) = maybe_batch_commitment {
                                            let message = NetworkMessage::ApproveCommitments {
                                                batch_commitment,
                                            };
                                            n.publish_message(message.encode());
                                        }
                                    }
                                }
                                NetworkMessage::ApproveCommitments {
                                    batch_commitment: (digest, signature),
                                } => {
                                    if let Some(s) = sequencer.as_mut() {
                                        let _ = s.receive_batch_commitment_signature(digest, signature)
                                        .inspect_err(|e| log::warn!("failed to receive batch commitment signature from network: {e}"));
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
                        _ => {}
                    }
                }
                ServiceEvent::Observer(event) => match event {
                    ObserverEvent::Blob {
                        code_id,
                        timestamp,
                        code,
                    } => compute.receive_code(code_id, timestamp, code),
                    ObserverEvent::Block(block_data) => compute.receive_chain_head(block_data),
                },
                ServiceEvent::Prometheus(event) => {
                    let Some(p) = prometheus.as_mut() else {
                        unreachable!("couldn't produce event without prometheus");
                    };

                    match event {
                        PrometheusEvent::CollectMetrics => {
                            let status = observer.status();

                            p.update_observer_metrics(status.eth_best_height, status.pending_codes);

                            if let Some(s) = sequencer.as_ref() {
                                let status = s.status();

                                p.update_sequencer_metrics(
                                    status.submitted_code_commitments,
                                    status.submitted_block_commitments,
                                );
                            };
                        }
                    }
                }
                ServiceEvent::Rpc(event) => log::info!("Received RPC event: {event:#?}"),
                ServiceEvent::Sequencer(event) => {
                    let Some(s) = sequencer.as_mut() else {
                        unreachable!("couldn't produce event without sequencer");
                    };

                    match event {
                        SequencerEvent::CollectionRoundEnded { block_hash: _ } => {
                            let code_requests: Vec<_> =
                                s.get_candidate_code_commitments().cloned().collect();

                            let block_requests: Vec<_> = s
                                .get_candidate_block_commitments()
                                .map(BlockCommitmentValidationRequest::from)
                                .collect();

                            if let Some(n) = network.as_mut() {
                                // TODO (breathx): remove this clones bypassing as call arguments by ref: anyway we encode.
                                let message = NetworkMessage::RequestCommitmentsValidation {
                                    codes: code_requests.clone(),
                                    blocks: block_requests.clone(),
                                };

                                log::debug!(
                                    "Request validation of aggregated commitments: {message:?}"
                                );

                                n.publish_message(message.encode());
                            };

                            if let Some(v) = validator.as_mut() {
                                log::debug!(
                                    "Validate collected ({}) code commitments and ({}) block commitments...",
                                    code_requests.len(),
                                    block_requests.len()
                                );

                                // Because sequencer can collect commitments from different sources,
                                // it's possible that some of collected commitments validation will fail
                                // on local validator. So we just print warning in this case.

                                if !code_requests.is_empty() || !block_requests.is_empty() {
                                    match v.validate_batch_commitment(
                                        &db,
                                        code_requests,
                                        block_requests,
                                    ) {
                                        Ok((digest, signature)) => {
                                            s.receive_batch_commitment_signature(
                                                digest, signature,
                                            )?;
                                        }
                                        Err(err) => {
                                            log::warn!("Collected batch commitments validation failed: {err}");
                                        }
                                    }
                                }
                            };
                        }
                        SequencerEvent::ValidationRoundEnded { .. } => {}
                        SequencerEvent::CommitmentSubmitted { .. } => {}
                    }
                }
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
}

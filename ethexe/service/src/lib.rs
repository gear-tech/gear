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
use ethexe_common::{
    events::{BlockEvent, BlockRequestEvent, RouterRequestEvent},
    gear::{BlockCommitment, CodeCommitment, StateTransition},
};
use ethexe_db::{BlockMetaStorage, CodesStorage, Database};
use ethexe_ethereum::router::RouterQuery;
use ethexe_network::{db_sync, NetworkEvent, NetworkService};
use ethexe_observer::{MockBlobReader, ObserverEvent, ObserverService, RequestBlockData};
use ethexe_processor::{LocalOutcome, ProcessorConfig};
use ethexe_prometheus::{PrometheusEvent, PrometheusService};
use ethexe_sequencer::{
    agro::AggregatedCommitments, SequencerConfig, SequencerEvent, SequencerService,
};
use ethexe_service_utils::{OptionFuture as _, OptionStreamNext as _};
use ethexe_signer::{Digest, PublicKey, Signature, Signer};
use ethexe_validator::BlockCommitmentValidationRequest;
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::{ops::Not, sync::Arc};

pub mod config;

#[cfg(test)]
mod tests;

/// ethexe service.
pub struct Service {
    db: Database,
    observer: ObserverService,
    query: ethexe_observer::Query,
    router_query: RouterQuery,
    processor: ethexe_processor::Processor,
    signer: ethexe_signer::Signer,

    // Optional services
    network: Option<NetworkService>,
    sequencer: Option<SequencerService>,
    validator: Option<ethexe_validator::Validator>,
    prometheus: Option<PrometheusService>,
    rpc: Option<ethexe_rpc::RpcService>,
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
        codes: Option<(Digest, Signature)>,
        blocks: Option<(Digest, Signature)>,
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

        let validator = Self::get_config_public_key(config.node.validator, &signer)
            .with_context(|| "failed to get validator private key")?
            .map(|key| {
                ethexe_validator::Validator::new(
                    &ethexe_validator::Config {
                        pub_key: key,
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

        Ok(Self {
            db,
            network,
            observer,
            query,
            router_query,
            processor,
            sequencer,
            signer,
            validator,
            prometheus,
            rpc,
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
    ) -> Self {
        Self {
            db,
            observer,
            query,
            router_query,
            processor,
            signer,
            network,
            sequencer,
            validator,
            prometheus,
            rpc,
        }
    }

    // TODO: remove this function.
    // This is a temporary solution to download absent codes from already processed blocks.
    async fn process_upload_codes(
        db: &Database,
        query: &mut ethexe_observer::Query,
        processor: &mut ethexe_processor::Processor,
        block_hash: H256,
    ) -> Result<()> {
        let events = query.get_block_request_events(block_hash).await?;

        for event in events {
            match event {
                BlockRequestEvent::Router(RouterRequestEvent::CodeValidationRequested {
                    code_id,
                    blob_tx_hash,
                }) => {
                    db.set_code_blob_tx(code_id, blob_tx_hash);
                }
                BlockRequestEvent::Router(RouterRequestEvent::ProgramCreated {
                    code_id, ..
                }) => {
                    if db.original_code(code_id).is_some() {
                        continue;
                    }

                    log::debug!("📥 downloading absent code: {code_id}");

                    let blob_tx_hash = db
                        .code_blob_tx(code_id)
                        .ok_or_else(|| anyhow!("Blob tx hash not found"))?;

                    let code = query.download_code(code_id, blob_tx_hash).await?;

                    processor.process_upload_code(code_id, code.as_slice())?;
                }
                _ => continue,
            }
        }

        Ok(())
    }

    async fn process_one_block(
        db: &Database,
        query: &mut ethexe_observer::Query,
        processor: &mut ethexe_processor::Processor,
        block_hash: H256,
    ) -> Result<Vec<StateTransition>> {
        if let Some(transitions) = db.block_outcome(block_hash) {
            return Ok(transitions);
        }

        query.propagate_meta_for_block(block_hash).await?;

        Self::process_upload_codes(db, query, processor, block_hash).await?;

        let block_request_events = query.get_block_request_events(block_hash).await?;

        let block_outcomes = processor.process_block_events(block_hash, block_request_events)?;

        let transition_outcomes: Vec<_> = block_outcomes
            .into_iter()
            .map(|outcome| {
                if let LocalOutcome::Transition(transition) = outcome {
                    transition
                } else {
                    unreachable!("Only transitions are expected here")
                }
            })
            .collect();

        db.set_block_is_empty(block_hash, transition_outcomes.is_empty());
        if !transition_outcomes.is_empty() {
            // Not empty blocks must be committed,
            // so append it to the `wait for commitment` queue.
            let mut queue = db
                .block_commitment_queue(block_hash)
                .ok_or_else(|| anyhow!("Commitment queue is not found for block"))?;
            queue.push_back(block_hash);
            db.set_block_commitment_queue(block_hash, queue);
        }

        db.set_block_outcome(block_hash, transition_outcomes.clone());

        // Set block as valid - means state db has all states for the end of the block
        db.set_block_end_state_is_valid(block_hash, true);

        let header = db.block_header(block_hash).expect("must be set; qed");
        db.set_latest_valid_block(block_hash, header);

        Ok(transition_outcomes)
    }

    async fn process_block_event(
        db: &Database,
        query: &mut ethexe_observer::Query,
        processor: &mut ethexe_processor::Processor,
        block_data: RequestBlockData,
    ) -> Result<Vec<BlockCommitment>> {
        db.set_block_events(block_data.hash, block_data.events);
        db.set_block_header(block_data.hash, block_data.header);

        let mut commitments = vec![];

        let last_committed_chain = query.get_last_committed_chain(block_data.hash).await?;

        for block_hash in last_committed_chain.into_iter().rev() {
            let transitions = Self::process_one_block(db, query, processor, block_hash).await?;

            if transitions.is_empty() {
                // Skip empty blocks
                continue;
            }

            let header = db
                .block_header(block_hash)
                .ok_or_else(|| anyhow!("header not found, but most exist"))?;

            commitments.push(BlockCommitment {
                hash: block_hash,
                timestamp: header.timestamp,
                previous_committed_block: db
                    .previous_committed_block(block_hash)
                    .ok_or_else(|| anyhow!("Prev commitment not found"))?,
                predecessor_block: block_data.hash,
                transitions,
            });
        }

        Ok(commitments)
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
            mut query,
            mut router_query,
            mut processor,
            mut sequencer,
            signer: _signer,
            mut validator,
            mut prometheus,
            rpc,
        } = self;
        let mut rpc_handle = if let Some(rpc) = rpc {
            log::info!("🌐 Rpc server starting at: {}", rpc.port());

            let rpc_run = rpc.run_server().await?;

            Some(tokio::spawn(rpc_run.stopped()))
        } else {
            None
        };

        let mut roles = "Observer".to_string();
        if let Some(seq) = sequencer.as_ref() {
            roles.push_str(&format!(", Sequencer ({})", seq.address()));
        }
        if let Some(val) = validator.as_ref() {
            roles.push_str(&format!(", Validator ({})", val.address()));
        }
        log::info!("⚙️ Node service starting, roles: [{}]", roles);

        loop {
            tokio::select! {
                event = observer.next() => {
                    match event? {
                        ObserverEvent::Blob { code_id, code } => {
                            // TODO: spawn blocking here?
                            let valid = processor.process_upload_code_raw(code_id, code.as_slice())?;

                            let code_commitments = vec![CodeCommitment { id: code_id, valid }];

                            if let Some(v) = validator.as_mut() {
                                let aggregated_code_commitments = v.aggregate(code_commitments)?;

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
                        },
                        ObserverEvent::Block(block) => {
                            let hash = block.hash;

                            log::info!(
                                "📦 receive a new block {}, hash {hash}, parent hash {}",
                                block.header.height,
                                block.header.parent_hash
                            );

                            let block = RequestBlockData {
                                hash,
                                header: block.header,
                                events: block.events.into_iter().flat_map(BlockEvent::to_request).collect(),
                            };

                            // TODO: spawn blocking here?
                            let block_commitments =
                                Self::process_block_event(&db, &mut query, &mut processor, block).await?;

                            if let Some(s) = sequencer.as_mut() {
                                s.on_new_head(hash)?
                            }

                            if block_commitments.is_empty() {
                                continue;
                            }

                            if let Some(v) = validator.as_mut() {
                                let aggregated_block_commitments = v.aggregate(block_commitments)?;

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
                    }
                },
                Some(event) = sequencer.maybe_next() => {
                    let Some(s) = sequencer.as_mut() else {
                        unreachable!("couldn't produce event without sequencer");
                    };

                    match event {
                        SequencerEvent::CollectionRoundEnded { block_hash: _ } => {
                            let code_requests: Vec<_> = s
                                .get_candidate_code_commitments()
                                .cloned()
                                .collect();

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

                                log::debug!("Request validation of aggregated commitments: {message:?}");

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

                                if !code_requests.is_empty() {
                                    match v.validate_code_commitments(&db, code_requests) {
                                        Ok((digest, signature)) => {
                                            s.receive_codes_signature(digest, signature)?;
                                        }
                                        Err(err) => {
                                            log::warn!("Collected code commitments validation failed: {err}");
                                        }
                                    }
                                };

                                if !block_requests.is_empty() {
                                    match v.validate_block_commitments(&db, block_requests) {
                                        Ok((digest, signature)) => {
                                            s.receive_blocks_signature(digest, signature)?;
                                        }
                                        Err(err) => {
                                            log::warn!("Collected block commitments validation failed: {err}");
                                        }
                                    }
                                };
                            };
                        },
                        SequencerEvent::ValidationRoundEnded { .. } => {},
                    }
                },
                Some(event) = network.maybe_next() => {
                    match event {
                        NetworkEvent::Message { source, data } => {
                            log::trace!("Received a network message from peer {source:?}");

                            let Ok(message) = NetworkMessage::decode(&mut data.as_slice())
                                .inspect_err(|e| log::warn!("Failed to decode network message: {e}"))
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
                                },
                                NetworkMessage::RequestCommitmentsValidation { codes, blocks } => {
                                    if let Some(v) = validator.as_mut() {
                                        let codes = codes
                                            .is_empty()
                                            .not()
                                            .then(|| v.validate_code_commitments(&db, codes))
                                            .transpose()
                                            .inspect_err(|e| log::warn!("failed to validate code commitments from network: {e}"))
                                            .ok()
                                            .flatten();

                                        let blocks = blocks
                                            .is_empty()
                                            .not()
                                            .then(|| v.validate_block_commitments(&db, blocks))
                                            .transpose()
                                            .inspect_err(|e| log::warn!("failed to validate block commitments from network: {e}"))
                                            .ok()
                                            .flatten();

                                        if let Some(n) = network.as_mut() {
                                            let message = NetworkMessage::ApproveCommitments { codes, blocks };
                                            n.publish_message(message.encode());
                                        }
                                    }
                                },
                                NetworkMessage::ApproveCommitments { codes, blocks } => {
                                    if let Some(s) = sequencer.as_mut() {
                                        if let Some((digest, signature)) = codes {
                                            let _ = s.receive_codes_signature(digest, signature)
                                                .inspect_err(|e| log::warn!("failed to receive codes signature from network: {e}"));
                                        }

                                        if let Some((digest, signature)) = blocks {
                                            let _ = s.receive_blocks_signature(digest, signature)
                                                .inspect_err(|e| log::warn!("failed to receive blocks signature from network: {e}"));
                                        }
                                    }
                                },
                            };
                        }
                        NetworkEvent::ExternalValidation(validating_response) => {
                            let validated = Self::process_response_validation(&validating_response, &mut router_query).await?;
                            let res = if validated {
                                Ok(validating_response)
                            } else {
                                Err(validating_response)
                            };

                            network
                                .as_mut()
                                .expect("if network receiver is `Some()` so does sender")
                                .request_validated(res);
                        }
                        _ => {}
                    }},
                Some(event) = prometheus.maybe_next() => {
                    let Some(p) = prometheus.as_mut() else {
                        unreachable!("couldn't produce event without prometheus");
                    };

                    match event {
                        PrometheusEvent::CollectMetrics => {
                            let status = observer.status();

                            p.update_observer_metrics(
                                status.eth_best_height,
                                status.pending_codes,
                            );

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
                _ = rpc_handle.as_mut().maybe() => {
                    log::info!("`RPCWorker` has terminated, shutting down...");
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

// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

//! Main service in ethexe node.

use crate::{
    config::{Config, ConfigPublicKey, PrometheusConfig},
    metrics::MetricsService,
};
use anyhow::{anyhow, Ok, Result};
use ethexe_common::{
    events::BlockEvent, BlockCommitment, CodeCommitment, Commitments, StateTransition,
};
use ethexe_db::{BlockHeader, BlockMetaStorage, CodeUploadInfo, CodesStorage, Database};
use ethexe_network::GossipsubMessage;
use ethexe_observer::{BlockData, CodeLoadedData};
use ethexe_processor::LocalOutcome;
use ethexe_sequencer::NetworkMessage;
use ethexe_signer::{AsDigest, PublicKey, Signer};
use futures::{future, stream::StreamExt, FutureExt};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::{collections::BTreeMap, future::Future, ops::Not, sync::Arc, time::Duration};

/// ethexe service.
pub struct Service {
    db: Database,
    observer: ethexe_observer::Observer,
    query: ethexe_observer::Query,
    processor: ethexe_processor::Processor,
    signer: ethexe_signer::Signer,
    block_time: Duration,

    // Optional services
    network: Option<ethexe_network::NetworkService>,
    sequencer: Option<ethexe_sequencer::Sequencer>,
    validator: Option<ethexe_validator::Validator>,
    metrics_service: Option<MetricsService>,
    rpc: Option<ethexe_rpc::RpcService>,
}

impl Service {
    pub async fn new(config: &Config) -> Result<Self> {
        let rocks_db = ethexe_db::RocksDatabase::open(config.database_path.clone())?;
        let db = ethexe_db::Database::from_one(&rocks_db);

        let blob_reader = Arc::new(
            ethexe_observer::ConsensusLayerBlobReader::new(
                &config.ethereum_rpc,
                &config.ethereum_beacon_rpc,
                config.block_time,
            )
            .await?,
        );

        let ethereum_router_address = config.ethereum_router_address;
        let observer = ethexe_observer::Observer::new(
            &config.ethereum_rpc,
            ethereum_router_address,
            blob_reader.clone(),
        )
        .await?;

        let router_query =
            ethexe_ethereum::RouterQuery::new(&config.ethereum_rpc, ethereum_router_address)
                .await?;
        let genesis_block_hash = router_query.genesis_block_hash().await?;
        log::info!("👶 Genesis block hash: {genesis_block_hash}");

        let query = ethexe_observer::Query::new(
            Arc::new(db.clone()),
            &config.ethereum_rpc,
            ethereum_router_address,
            genesis_block_hash,
            blob_reader,
            config.max_commitment_depth,
        )
        .await?;

        let processor = ethexe_processor::Processor::new(db.clone())?;

        let signer = ethexe_signer::Signer::new(config.key_path.clone())?;

        let sequencer = if let Some(key) = Self::get_config_public_key(config.sequencer, &signer)? {
            Some(
                ethexe_sequencer::Sequencer::new(
                    &ethexe_sequencer::Config {
                        ethereum_rpc: config.ethereum_rpc.clone(),
                        sign_tx_public: key,
                        router_address: config.ethereum_router_address,
                        validators: config.validators.clone(),
                    },
                    signer.clone(),
                )
                .await?,
            )
        } else {
            None
        };

        let validator = Self::get_config_public_key(config.validator, &signer)?.map(|key| {
            ethexe_validator::Validator::new(
                &ethexe_validator::Config {
                    pub_key: key,
                    router_address: config.ethereum_router_address,
                },
                signer.clone(),
            )
        });

        // Prometheus metrics.
        let metrics_service =
            if let Some(PrometheusConfig { port, registry }) = config.prometheus_config.clone() {
                // Set static metrics.
                let metrics = MetricsService::with_prometheus(&registry, config)?;
                tokio::spawn(ethexe_prometheus_endpoint::init_prometheus(port, registry).map(drop));

                Some(metrics)
            } else {
                None
            };

        let network = config
            .net_config
            .as_ref()
            .map(|config| -> Result<_> {
                ethexe_network::NetworkService::new(config.clone(), &signer)
            })
            .transpose()?;

        let rpc = config
            .rpc_port
            .map(|port| ethexe_rpc::RpcService::new(port, db.clone()));

        Ok(Self {
            db,
            network,
            observer,
            query,
            processor,
            sequencer,
            signer,
            validator,
            metrics_service,
            rpc,
            block_time: config.block_time,
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
        observer: ethexe_observer::Observer,
        query: ethexe_observer::Query,
        processor: ethexe_processor::Processor,
        signer: ethexe_signer::Signer,
        block_time: Duration,
        network: Option<ethexe_network::NetworkService>,
        sequencer: Option<ethexe_sequencer::Sequencer>,
        validator: Option<ethexe_validator::Validator>,
        metrics_service: Option<MetricsService>,
        rpc: Option<ethexe_rpc::RpcService>,
    ) -> Self {
        Self {
            db,
            observer,
            query,
            processor,
            signer,
            block_time,
            network,
            sequencer,
            validator,
            metrics_service,
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
        let events = query.get_block_events(block_hash).await?;

        for event in events {
            match event {
                BlockEvent::UploadCode(event) => {
                    db.set_code_upload_info(
                        event.code_id,
                        CodeUploadInfo {
                            origin: event.origin,
                            tx_hash: event.blob_tx(),
                        },
                    );
                }
                BlockEvent::CreateProgram(event) => {
                    let code_id = event.code_id;
                    if db.original_code(code_id).is_some() {
                        continue;
                    }

                    log::debug!("📥 downloading absent code: {code_id}");
                    let CodeUploadInfo { origin, tx_hash } = db
                        .code_upload_info(code_id)
                        .ok_or(anyhow!("Origin and tx hash not found"))?;
                    let code = query.download_code(code_id, origin, tx_hash).await?;
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

        let block_events = query.get_block_events(block_hash).await?;

        let block_outcomes = processor.process_block_events(block_hash, &block_events)?;

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
                .ok_or(anyhow!("Commitment queue is not found for block"))?;
            queue.push_back(block_hash);
            db.set_block_commitment_queue(block_hash, queue);
        }

        db.set_block_outcome(block_hash, transition_outcomes.clone());

        // Set block as valid - means state db has all states for the end of the block
        db.set_block_end_state_is_valid(block_hash, true);

        Ok(transition_outcomes)
    }

    async fn process_block_event(
        db: &Database,
        query: &mut ethexe_observer::Query,
        processor: &mut ethexe_processor::Processor,
        block_data: BlockData,
    ) -> Result<Vec<BlockCommitment>> {
        db.set_block_events(block_data.block_hash, block_data.events.clone());
        db.set_block_header(
            block_data.block_hash,
            BlockHeader {
                height: block_data.block_number.try_into()?,
                timestamp: block_data.block_timestamp,
                parent_hash: block_data.parent_hash,
            },
        );

        let mut commitments = vec![];
        let last_committed_chain = query
            .get_last_committed_chain(block_data.block_hash)
            .await?;
        for block_hash in last_committed_chain.into_iter().rev() {
            let transitions = Self::process_one_block(db, query, processor, block_hash).await?;

            if transitions.is_empty() {
                // Skip empty blocks
                continue;
            }

            commitments.push(BlockCommitment {
                block_hash,
                allowed_pred_block_hash: block_data.block_hash,
                allowed_prev_commitment_hash: db
                    .block_prev_commitment(block_hash)
                    .ok_or(anyhow!("Prev commitment not found"))?,
                transitions,
            });
        }

        Ok(commitments)
    }

    async fn process_observer_event(
        db: &Database,
        query: &mut ethexe_observer::Query,
        processor: &mut ethexe_processor::Processor,
        maybe_sequencer: &mut Option<ethexe_sequencer::Sequencer>,
        observer_event: ethexe_observer::Event,
    ) -> Result<Commitments> {
        if let Some(sequencer) = maybe_sequencer {
            sequencer.process_observer_event(&observer_event)?;
        }

        match observer_event {
            ethexe_observer::Event::Block(block_data) => {
                log::info!(
                    "📦 receive a new block {}, hash {}, parent hash {}",
                    block_data.block_number,
                    block_data.block_hash,
                    block_data.parent_hash
                );
                let commitments =
                    Self::process_block_event(db, query, processor, block_data).await?;
                Ok((Vec::new(), commitments).into())
            }
            ethexe_observer::Event::CodeLoaded(CodeLoadedData { code_id, code, .. }) => {
                let outcomes = processor.process_upload_code(code_id, code.as_slice())?;
                let commitments: Vec<_> = outcomes
                    .into_iter()
                    .map(|outcome| match outcome {
                        LocalOutcome::CodeApproved(code_id) => CodeCommitment {
                            code_id,
                            approved: true,
                        },
                        LocalOutcome::CodeRejected(code_id) => CodeCommitment {
                            code_id,
                            approved: false,
                        },
                        _ => unreachable!("Only code outcomes are expected here"),
                    })
                    .collect();
                Ok((commitments, Vec::new()).into())
            }
        }
    }

    async fn run_inner(self) -> Result<()> {
        let Service {
            db,
            network,
            mut observer,
            mut query,
            mut processor,
            mut sequencer,
            signer: _signer,
            mut validator,
            metrics_service,
            rpc,
            block_time,
        } = self;

        if let Some(metrics_service) = metrics_service {
            tokio::spawn(metrics_service.run(
                observer.get_status_receiver(),
                sequencer.as_mut().map(|s| s.get_status_receiver()),
            ));
        }

        let observer_events = observer.events();
        futures::pin_mut!(observer_events);

        let (mut network_sender, mut gossipsub_stream, mut network_handle) =
            if let Some(network) = network {
                (
                    Some(network.sender),
                    Some(network.gossip_stream),
                    Some(tokio::spawn(network.event_loop.run())),
                )
            } else {
                (None, None, None)
            };

        let mut rpc_handle = if let Some(rpc) = rpc {
            let (rpc_run, rpc_port) = rpc.run_server().await?;
            log::info!("🌐 Rpc server started at: {}", rpc_port);
            Some(tokio::spawn(rpc_run.stopped()))
        } else {
            None
        };

        let mut collection_round_timer: Option<_> = None;
        let mut submission_round_timer: Option<_> = None;

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
                observer_event = observer_events.next() => {
                    let Some(observer_event) = observer_event else {
                        log::info!("Observer stream ended, shutting down...");
                        break;
                    };

                    let commitments = Self::process_observer_event(
                        &db,
                        &mut query,
                        &mut processor,
                        &mut sequencer,
                        observer_event,
                    ).await?;

                    Self::post_process_commitments(
                        commitments,
                        validator.as_mut(),
                        sequencer.as_mut(),
                        network_sender.as_mut(),
                    ).await?;

                    collection_round_timer = Some(tokio::time::sleep(block_time / 4));
                }
                _ = maybe_await(collection_round_timer.take()) => {
                    log::debug!("Reach timeout after block event...");

                    Self::process_collected_commitments(
                        db.clone(),
                        validator.as_mut(),
                        sequencer.as_mut(),
                        network_sender.as_mut()
                    )?;

                    submission_round_timer = Some(tokio::time::sleep(block_time / 4));
                }
                _ = maybe_await(submission_round_timer.take()) => {
                    log::debug!("Reach timeout after collecting commitments...");

                    Self::process_approved_commitments(sequencer.as_mut()).await?;
                }
                message = maybe_await(gossipsub_stream.as_mut().map(|stream| stream.next())) => {
                    let Some(message) = message else {
                        continue;
                    };

                    let _ = Self::process_network_message(
                        message,
                        db.clone(),
                        validator.as_mut(),
                        sequencer.as_mut(),
                        network_sender.as_mut(),
                    );
                }
                _ = maybe_await(network_handle.as_mut()) => {
                    log::info!("`NetworkWorker` has terminated, shutting down...");
                    break;
                }
                _ = maybe_await(rpc_handle.as_mut()) => {
                    log::info!("`RPCWorker` has terminated, shutting down...");
                    break;
                }
            }
        }

        Ok(())
    }

    pub async fn run(self) -> Result<()> {
        self.run_inner().await.map_err(|err| {
            log::error!("Service finished work with error: {:?}", err);
            err
        })
    }

    async fn post_process_commitments(
        commitments: Commitments,
        maybe_validator: Option<&mut ethexe_validator::Validator>,
        maybe_sequencer: Option<&mut ethexe_sequencer::Sequencer>,
        maybe_network_sender: Option<&mut ethexe_network::NetworkSender>,
    ) -> Result<()> {
        let Some(validator) = maybe_validator else {
            return Ok(());
        };

        let origin = validator.address();
        let aggregated_codes = commitments
            .codes
            .is_empty()
            .not()
            .then(|| validator.aggregate_codes(commitments.codes))
            .transpose()?;
        let aggregated_blocks = commitments
            .blocks
            .is_empty()
            .not()
            .then(|| validator.aggregate_blocks(commitments.blocks))
            .transpose()?;

        if aggregated_codes.is_none() && aggregated_blocks.is_none() {
            return Ok(());
        }

        if let Some(network_sender) = maybe_network_sender {
            log::debug!("Publishing commitments to network...");
            network_sender.publish_message(
                NetworkMessage::PublishCommitments {
                    origin,
                    codes: aggregated_codes.clone(),
                    blocks: aggregated_blocks.clone(),
                }
                .encode(),
            );
        }

        if let Some(sequencer) = maybe_sequencer {
            if let Some(aggregated) = aggregated_codes {
                log::debug!(
                    "Received ({}) signed code commitments from local validator...",
                    aggregated.len()
                );
                sequencer.receive_code_commitments(origin, aggregated)?;
            }
            if let Some(aggregated) = aggregated_blocks {
                log::debug!(
                    "Received ({}) signed block commitments from local validator...",
                    aggregated.len()
                );
                sequencer.receive_block_commitments(origin, aggregated)?;
            }
        }

        Ok(())
    }

    fn process_collected_commitments(
        db: Database,
        maybe_validator: Option<&mut ethexe_validator::Validator>,
        maybe_sequencer: Option<&mut ethexe_sequencer::Sequencer>,
        maybe_network_sender: Option<&mut ethexe_network::NetworkSender>,
    ) -> Result<()> {
        let Some(sequencer) = maybe_sequencer else {
            return Ok(());
        };

        let (codes_hash, blocks_hash) = sequencer.process_collected_commitments()?;

        if codes_hash.is_none() && blocks_hash.is_none() {
            return Ok(());
        }

        if maybe_validator.is_none() && maybe_network_sender.is_none() {
            return Ok(());
        }

        let code_requests: BTreeMap<_, _> = codes_hash
            .and_then(|hash| sequencer.get_multisigned_code_commitments(hash))
            .map(|commitments| {
                commitments
                    .iter()
                    .map(|c| (c.as_digest(), c.clone()))
                    .collect()
            })
            .unwrap_or_default();

        let block_requests: BTreeMap<_, _> = blocks_hash
            .and_then(|hash| sequencer.get_multisigned_block_commitments(hash))
            .map(|commitments| {
                commitments
                    .iter()
                    .map(|c| (c.as_digest(), c.into()))
                    .collect()
            })
            .unwrap_or_default();

        if let Some(network_sender) = maybe_network_sender {
            log::debug!("Request validation of aggregated commitments...");

            let message = NetworkMessage::RequestCommitmentsValidation {
                codes: code_requests.clone(),
                blocks: block_requests.clone(),
            };
            network_sender.publish_message(message.encode());
        }

        if let Some(validator) = maybe_validator {
            log::debug!("Validating aggregated commitments on local validator...");

            let origin = validator.address();

            if let Some(aggregated_hash) = codes_hash {
                let signature =
                    validator.validate_code_commitments(db.clone(), code_requests.into_values())?;
                sequencer.receive_codes_signature(origin, aggregated_hash, signature)?;
            }

            if let Some(aggregated_hash) = blocks_hash {
                let signature = validator
                    .validate_block_commitments(db.clone(), block_requests.into_values())?;
                sequencer.receive_blocks_signature(origin, aggregated_hash, signature)?;
            }
        }

        Ok(())
    }

    async fn process_approved_commitments(
        maybe_sequencer: Option<&mut ethexe_sequencer::Sequencer>,
    ) -> Result<()> {
        let Some(sequencer) = maybe_sequencer else {
            return Ok(());
        };

        sequencer.submit_multisigned_commitments().await
    }

    fn process_network_message(
        message: GossipsubMessage,
        db: Database,
        maybe_validator: Option<&mut ethexe_validator::Validator>,
        maybe_sequencer: Option<&mut ethexe_sequencer::Sequencer>,
        maybe_network_sender: Option<&mut ethexe_network::NetworkSender>,
    ) -> Result<()> {
        let message = NetworkMessage::decode(&mut message.data.as_slice())?;
        match message {
            NetworkMessage::PublishCommitments {
                origin,
                codes,
                blocks,
            } => {
                let Some(sequencer) = maybe_sequencer else {
                    return Ok(());
                };
                if let Some(aggregated) = codes {
                    sequencer.receive_code_commitments(origin, aggregated)?;
                }
                if let Some(aggregated) = blocks {
                    sequencer.receive_block_commitments(origin, aggregated)?;
                }
                Ok(())
            }
            NetworkMessage::RequestCommitmentsValidation { codes, blocks } => {
                let Some(validator) = maybe_validator else {
                    return Ok(());
                };
                let Some(network_sender) = maybe_network_sender else {
                    return Ok(());
                };

                let codes_signature = if let Some(codes_hash) = codes
                    .is_empty()
                    .not()
                    .then(|| codes.keys().cloned().collect::<Vec<_>>().as_digest())
                {
                    let signature =
                        validator.validate_code_commitments(db.clone(), codes.into_values())?;
                    Some((codes_hash, signature))
                } else {
                    None
                };

                let blocks_signature = if let Some(blocks_hash) = blocks
                    .is_empty()
                    .not()
                    .then(|| blocks.keys().cloned().collect::<Vec<_>>().as_digest())
                {
                    let signature =
                        validator.validate_block_commitments(db.clone(), blocks.into_values())?;
                    Some((blocks_hash, signature))
                } else {
                    None
                };

                let message = NetworkMessage::ApproveCommitments {
                    origin: validator.address(),
                    codes: codes_signature,
                    blocks: blocks_signature,
                };
                network_sender.publish_message(message.encode());

                Ok(())
            }
            NetworkMessage::ApproveCommitments {
                origin,
                codes,
                blocks,
            } => {
                let Some(sequencer) = maybe_sequencer else {
                    return Ok(());
                };

                if let Some((hash, signature)) = codes {
                    sequencer.receive_codes_signature(origin, hash, signature)?;
                }

                if let Some((hash, signature)) = blocks {
                    sequencer.receive_blocks_signature(origin, hash, signature)?;
                }

                Ok(())
            }
        }
    }
}

pub async fn maybe_await<F: Future>(f: Option<F>) -> F::Output {
    if let Some(f) = f {
        f.await
    } else {
        future::pending().await
    }
}

#[cfg(test)]
mod tests {
    use super::Service;
    use crate::config::{Config, PrometheusConfig};
    use std::{
        net::{Ipv4Addr, SocketAddr},
        time::Duration,
    };
    use tempfile::tempdir;

    #[tokio::test]
    async fn basics() {
        let tmp_dir = tempdir().unwrap();
        let tmp_dir = tmp_dir.path().to_path_buf();

        let net_path = tmp_dir.join("net");
        let net_config = ethexe_network::NetworkEventLoopConfig::new_local(net_path);

        Service::new(&Config {
            node_name: "test".to_string(),
            ethereum_rpc: "wss://ethereum-holesky-rpc.publicnode.com".into(),
            ethereum_beacon_rpc: "http://localhost:5052".into(),
            ethereum_router_address: "0x05069E9045Ca0D2B72840c6A21C7bE588E02089A"
                .parse()
                .expect("infallible"),
            max_commitment_depth: 1000,
            block_time: Duration::from_secs(1),
            database_path: tmp_dir.join("db"),
            key_path: tmp_dir.join("key"),
            sequencer: Default::default(),
            validator: Default::default(),
            sender_address: Default::default(),
            net_config: Some(net_config),
            prometheus_config: Some(PrometheusConfig::new_with_default_registry(
                SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9635),
                "dev".to_string(),
            )),
            rpc_port: Some(9090),
            validators: Default::default(),
        })
        .await
        .unwrap();

        // Disable all optional services
        Service::new(&Config {
            node_name: "test".to_string(),
            ethereum_rpc: "wss://ethereum-holesky-rpc.publicnode.com".into(),
            ethereum_beacon_rpc: "http://localhost:5052".into(),
            ethereum_router_address: "0x05069E9045Ca0D2B72840c6A21C7bE588E02089A"
                .parse()
                .expect("infallible"),
            max_commitment_depth: 1000,
            block_time: Duration::from_secs(1),
            database_path: tmp_dir.join("db"),
            key_path: tmp_dir.join("key"),
            sequencer: Default::default(),
            validator: Default::default(),
            sender_address: Default::default(),
            net_config: None,
            prometheus_config: None,
            rpc_port: None,
            validators: Default::default(),
        })
        .await
        .unwrap();
    }
}

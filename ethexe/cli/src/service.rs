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
    router::{
        BlockCommitment, CodeCommitment, RequestEvent as RouterRequestEvent, StateTransition,
    },
    BlockRequestEvent,
};
use ethexe_db::{BlockHeader, BlockMetaStorage, CodesStorage, Database};
use ethexe_ethereum::router::RouterQuery;
use ethexe_network::NetworkReceiverEvent;
use ethexe_observer::{RequestBlockData, RequestEvent};
use ethexe_processor::LocalOutcome;
use ethexe_sequencer::agro::AggregatedCommitments;
use ethexe_signer::{Digest, PublicKey, Signature, Signer};
use ethexe_validator::BlockCommitmentValidationRequest;
use futures::{future, stream::StreamExt, FutureExt};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::{
    future::Future,
    ops::Not,
    sync::Arc,
    time::{Duration, Instant},
};
use utils::*;

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
        let blob_reader = Arc::new(
            ethexe_observer::ConsensusLayerBlobReader::new(
                &config.ethereum_rpc,
                &config.ethereum_beacon_rpc,
                config.block_time,
            )
            .await?,
        );

        let ethereum_router_address = config.ethereum_router_address;
        let rocks_db = ethexe_db::RocksDatabase::open(config.database_path.clone())?;
        let db = ethexe_db::Database::from_one(&rocks_db, ethereum_router_address.0);

        let observer = ethexe_observer::Observer::new(
            &config.ethereum_rpc,
            ethereum_router_address,
            blob_reader.clone(),
        )
        .await?;

        let router_query = RouterQuery::new(&config.ethereum_rpc, ethereum_router_address).await?;

        let genesis_block_hash = router_query.genesis_block_hash().await?;
        log::info!("👶 Genesis block hash: {genesis_block_hash:?}");

        let validators = router_query.validators().await?;
        log::info!("👥 Validators set: {validators:?}");

        let threshold = router_query.threshold().await?;
        log::info!("🔒 Multisig threshold: {threshold} / {}", validators.len());

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
                        validators,
                        threshold,
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
                ethexe_network::NetworkService::new(config.clone(), &signer, db.clone())
            })
            .transpose()?;

        let rpc = config
            .rpc_port
            .map(|port| ethexe_rpc::RpcService::new(port, db.clone(), processor.clone()));

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
        db.set_block_events(block_data.block_hash, block_data.events);
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
                pred_block_hash: block_data.block_hash,
                prev_commitment_hash: db
                    .block_prev_commitment(block_hash)
                    .ok_or_else(|| anyhow!("Prev commitment not found"))?,
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
        observer_event: RequestEvent,
    ) -> Result<(Vec<CodeCommitment>, Vec<BlockCommitment>)> {
        if let Some(sequencer) = maybe_sequencer {
            sequencer.process_observer_event(&observer_event)?;
        }

        match observer_event {
            RequestEvent::Block(block_data) => {
                log::info!(
                    "📦 receive a new block {}, hash {}, parent hash {}",
                    block_data.block_number,
                    block_data.block_hash,
                    block_data.parent_hash
                );

                let commitments =
                    Self::process_block_event(db, query, processor, block_data).await?;

                Ok((Vec::new(), commitments))
            }
            RequestEvent::CodeLoaded { code_id, code } => {
                let outcomes = processor.process_upload_code(code_id, code.as_slice())?;
                let commitments: Vec<_> = outcomes
                    .into_iter()
                    .map(|outcome| match outcome {
                        LocalOutcome::CodeValidated { id, valid } => CodeCommitment { id, valid },
                        _ => unreachable!("Only code outcomes are expected here"),
                    })
                    .collect();
                Ok((commitments, Vec::new()))
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

        let observer_events = observer.request_events();
        futures::pin_mut!(observer_events);

        let (mut network_sender, mut network_receiver, mut network_handle) =
            if let Some(network) = network {
                (
                    Some(network.sender),
                    Some(network.receiver),
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

        let mut roles = "Observer".to_string();
        if let Some(seq) = sequencer.as_ref() {
            roles.push_str(&format!(", Sequencer ({})", seq.address()));
        }
        if let Some(val) = validator.as_ref() {
            roles.push_str(&format!(", Validator ({})", val.address()));
        }
        log::info!("⚙️ Node service starting, roles: [{}]", roles);

        let mut collection_round_timer = StoppableTimer::new(block_time / 4);
        let mut validation_round_timer = StoppableTimer::new(block_time / 4);

        loop {
            tokio::select! {
                observer_event = observer_events.next() => {
                    let Some(observer_event) = observer_event else {
                        log::info!("Observer stream ended, shutting down...");
                        break;
                    };

                    let is_block_event = matches!(observer_event, RequestEvent::Block(_));

                    let (code_commitments, block_commitments) = Self::process_observer_event(
                        &db,
                        &mut query,
                        &mut processor,
                        &mut sequencer,
                        observer_event,
                    ).await?;

                    Self::post_process_commitments(
                        code_commitments,
                        block_commitments,
                        validator.as_mut(),
                        sequencer.as_mut(),
                        network_sender.as_mut(),
                    ).await?;

                    if is_block_event {
                        collection_round_timer.start();
                        validation_round_timer.stop();
                    }
                }
                _ = collection_round_timer.wait() => {
                    log::debug!("Collection round timeout, process collected commitments...");

                    Self::process_collected_commitments(
                        &db,
                        validator.as_mut(),
                        sequencer.as_mut(),
                        network_sender.as_mut()
                    )?;

                    collection_round_timer.stop();
                    validation_round_timer.start();
                }
                _ = validation_round_timer.wait() => {
                    log::debug!("Validation round timeout, process validated commitments...");

                    Self::process_approved_commitments(sequencer.as_mut()).await?;

                    validation_round_timer.stop();
                }
                event = maybe_await(network_receiver.as_mut().map(|rx| rx.recv())) => {
                    let Some(NetworkReceiverEvent::Message { source, data }) = event else {
                        continue;
                    };

                    log::debug!("Received a network message from peer {source:?}");

                    let result = Self::process_network_message(
                        data.as_slice(),
                        &db,
                        validator.as_mut(),
                        sequencer.as_mut(),
                        network_sender.as_mut(),
                    );

                    if let Err(err) = result {
                        // TODO: slash peer/validator in case of error #4175
                        // TODO: consider error log as temporary solution #4175
                        log::warn!("Failed to process network message: {err}");
                    }
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
        code_commitments: Vec<CodeCommitment>,
        block_commitments: Vec<BlockCommitment>,
        maybe_validator: Option<&mut ethexe_validator::Validator>,
        maybe_sequencer: Option<&mut ethexe_sequencer::Sequencer>,
        maybe_network_sender: Option<&mut ethexe_network::NetworkSender>,
    ) -> Result<()> {
        let Some(validator) = maybe_validator else {
            return Ok(());
        };

        if maybe_network_sender.is_none() && maybe_sequencer.is_none() {
            return Ok(());
        }

        let aggregated_codes = code_commitments
            .is_empty()
            .not()
            .then(|| validator.aggregate(code_commitments))
            .transpose()?;
        let aggregated_blocks = block_commitments
            .is_empty()
            .not()
            .then(|| validator.aggregate(block_commitments))
            .transpose()?;

        if aggregated_codes.is_none() && aggregated_blocks.is_none() {
            return Ok(());
        }

        if let Some(network_sender) = maybe_network_sender {
            log::debug!("Publishing commitments to network...");
            network_sender.publish_message(
                NetworkMessage::PublishCommitments {
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
                sequencer.receive_code_commitments(aggregated)?;
            }
            if let Some(aggregated) = aggregated_blocks {
                log::debug!(
                    "Received ({}) signed block commitments from local validator...",
                    aggregated.len()
                );
                sequencer.receive_block_commitments(aggregated)?;
            }
        }

        Ok(())
    }

    fn process_collected_commitments(
        db: &Database,
        maybe_validator: Option<&mut ethexe_validator::Validator>,
        maybe_sequencer: Option<&mut ethexe_sequencer::Sequencer>,
        maybe_network_sender: Option<&mut ethexe_network::NetworkSender>,
    ) -> Result<()> {
        let Some(sequencer) = maybe_sequencer else {
            return Ok(());
        };

        let Some(chain_head) = sequencer.chain_head() else {
            return Err(anyhow!("Chain head is not set in sequencer"));
        };

        // If chain head is not yet processed by this node, this is normal situation,
        // so we just skip this round for sequencer.

        let Some(block_is_empty) = db.block_is_empty(chain_head) else {
            log::warn!("Failed to get block emptiness status for {chain_head}");
            return Ok(());
        };

        let last_not_empty_block = match block_is_empty {
            true => match db.block_prev_commitment(chain_head) {
                Some(prev_commitment) => prev_commitment,
                None => {
                    log::warn!("Failed to get previous commitment for {chain_head}");
                    return Ok(());
                }
            },
            false => chain_head,
        };

        sequencer.process_collected_commitments(last_not_empty_block)?;

        if maybe_validator.is_none() && maybe_network_sender.is_none() {
            return Ok(());
        }

        let code_requests: Vec<_> = sequencer
            .get_candidate_code_commitments()
            .cloned()
            .collect();

        let block_requests: Vec<_> = sequencer
            .get_candidate_block_commitments()
            .map(BlockCommitmentValidationRequest::from)
            .collect();

        if let Some(network_sender) = maybe_network_sender {
            log::debug!("Request validation of aggregated commitments...");

            let message = NetworkMessage::RequestCommitmentsValidation {
                codes: code_requests.clone(),
                blocks: block_requests.clone(),
            };
            network_sender.publish_message(message.encode());
        }

        if let Some(validator) = maybe_validator {
            log::debug!(
                "Validate collected ({}) code commitments and ({}) block commitments...",
                code_requests.len(),
                block_requests.len()
            );

            // Because sequencer can collect commitments from different sources,
            // it's possible that some of collected commitments validation will fail
            // on local validator. So we just print warning in this case.

            if code_requests.is_empty().not() {
                match validator.validate_code_commitments(db, code_requests) {
                    Result::Ok((digest, signature)) => {
                        sequencer.receive_codes_signature(digest, signature)?
                    }
                    Result::Err(err) => {
                        log::warn!("Collected code commitments validation failed: {err}")
                    }
                }
            }

            if block_requests.is_empty().not() {
                match validator.validate_block_commitments(db, block_requests) {
                    Result::Ok((digest, signature)) => {
                        sequencer.receive_blocks_signature(digest, signature)?
                    }
                    Result::Err(err) => {
                        log::warn!("Collected block commitments validation failed: {err}")
                    }
                }
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
        mut data: &[u8],
        db: &Database,
        maybe_validator: Option<&mut ethexe_validator::Validator>,
        maybe_sequencer: Option<&mut ethexe_sequencer::Sequencer>,
        maybe_network_sender: Option<&mut ethexe_network::NetworkSender>,
    ) -> Result<()> {
        let message = NetworkMessage::decode(&mut data)?;
        match message {
            NetworkMessage::PublishCommitments { codes, blocks } => {
                let Some(sequencer) = maybe_sequencer else {
                    return Ok(());
                };
                if let Some(aggregated) = codes {
                    sequencer.receive_code_commitments(aggregated)?;
                }
                if let Some(aggregated) = blocks {
                    sequencer.receive_block_commitments(aggregated)?;
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

                let codes = codes
                    .is_empty()
                    .not()
                    .then(|| validator.validate_code_commitments(db, codes))
                    .transpose()?;

                let blocks = blocks
                    .is_empty()
                    .not()
                    .then(|| validator.validate_block_commitments(db, blocks))
                    .transpose()?;

                let message = NetworkMessage::ApproveCommitments { codes, blocks };
                network_sender.publish_message(message.encode());

                Ok(())
            }
            NetworkMessage::ApproveCommitments { codes, blocks } => {
                let Some(sequencer) = maybe_sequencer else {
                    return Ok(());
                };

                if let Some((digest, signature)) = codes {
                    sequencer.receive_codes_signature(digest, signature)?;
                }

                if let Some((digest, signature)) = blocks {
                    sequencer.receive_blocks_signature(digest, signature)?;
                }

                Ok(())
            }
        }
    }
}

mod utils {
    use super::*;

    pub(crate) struct StoppableTimer {
        round_duration: Duration,
        started: Option<Instant>,
    }

    impl StoppableTimer {
        pub fn new(round_duration: Duration) -> Self {
            Self {
                round_duration,
                started: None,
            }
        }

        pub fn start(&mut self) {
            self.started = Some(Instant::now());
        }

        pub fn stop(&mut self) {
            self.started = None;
        }

        pub async fn wait(&self) {
            maybe_await(self.remaining().map(|rem| tokio::time::sleep(rem))).await;
        }

        fn remaining(&self) -> Option<Duration> {
            self.started.map(|started| {
                let elapsed = started.elapsed();
                if elapsed < self.round_duration {
                    self.round_duration - elapsed
                } else {
                    Duration::ZERO
                }
            })
        }
    }

    pub(crate) async fn maybe_await<F: Future>(f: Option<F>) -> F::Output {
        if let Some(f) = f {
            f.await
        } else {
            future::pending().await
        }
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
        gear_utils::init_default_logger();

        let tmp_dir = tempdir().unwrap();
        let tmp_dir = tmp_dir.path().to_path_buf();

        let net_path = tmp_dir.join("net");
        let net_config = ethexe_network::NetworkEventLoopConfig::new_local(net_path);

        Service::new(&Config {
            node_name: "test".to_string(),
            ethereum_rpc: "ws://54.67.75.1:8546".into(),
            ethereum_beacon_rpc: "http://localhost:5052".into(),
            ethereum_router_address: "0xa9e7B594e18e28b1Cc0FA4000D92ded887CB356F"
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
        })
        .await
        .unwrap();

        // Disable all optional services
        Service::new(&Config {
            node_name: "test".to_string(),
            ethereum_rpc: "wss://ethereum-holesky-rpc.publicnode.com".into(),
            ethereum_beacon_rpc: "http://localhost:5052".into(),
            ethereum_router_address: "0xa9e7B594e18e28b1Cc0FA4000D92ded887CB356F"
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
        })
        .await
        .unwrap();
    }
}

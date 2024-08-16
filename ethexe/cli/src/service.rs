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
    router::{BlockCommitment, CodeCommitment, Event as RouterEvent, StateTransition},
    BlockEvent,
};
use ethexe_db::{BlockHeader, BlockMetaStorage, CodesStorage, Database};
use ethexe_ethereum::router::RouterQuery;
use ethexe_network::NetworkReceiverEvent;
use ethexe_observer::{BlockData, Event as ObserverEvent};
use ethexe_processor::LocalOutcome;
use ethexe_signer::{PublicKey, Signer};
use ethexe_validator::Commitment;
use futures::{future, stream::StreamExt, FutureExt};
use gprimitives::H256;
use parity_scale_codec::Decode;
use std::{future::Future, sync::Arc, time::Duration};

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
                BlockEvent::Router(RouterEvent::CodeValidationRequested {
                    code_id,
                    blob_tx_hash,
                }) => {
                    db.set_code_blob_tx(code_id, blob_tx_hash);
                }
                BlockEvent::Router(RouterEvent::ProgramCreated { code_id, .. }) => {
                    if db.original_code(code_id).is_some() {
                        continue;
                    }

                    log::debug!("📥 downloading absent code: {code_id}");

                    let blob_tx_hash = db
                        .code_blob_tx(code_id)
                        .ok_or(anyhow!("Blob tx hash not found"))?;

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

        let header = db.block_header(block_hash).expect("must be set; qed");
        db.set_latest_valid_block_height(header.height);

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
                pred_block_hash: block_data.block_hash,
                prev_commitment_hash: db
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
    ) -> Result<Vec<Commitment>> {
        if let Some(sequencer) = maybe_sequencer {
            sequencer.process_observer_event(&observer_event)?;
        }

        let commitments = match observer_event {
            ObserverEvent::Block(block_data) => {
                log::info!(
                    "📦 receive a new block {}, hash {}, parent hash {}",
                    block_data.block_number,
                    block_data.block_hash,
                    block_data.parent_hash
                );

                let commitments =
                    Self::process_block_event(db, query, processor, block_data).await?;

                commitments.into_iter().map(Commitment::Block).collect()
            }
            ethexe_observer::Event::CodeLoaded { code_id, code } => {
                let outcomes = processor.process_upload_code(code_id, code.as_slice())?;

                outcomes
                    .into_iter()
                    .map(|outcome| match outcome {
                        LocalOutcome::CodeApproved(code_id) => Commitment::Code(CodeCommitment {
                            id: code_id,
                            valid: true,
                        }),
                        LocalOutcome::CodeRejected(code_id) => Commitment::Code(CodeCommitment {
                            id: code_id,
                            valid: false,
                        }),
                        _ => unreachable!("Only code outcomes are expected here"),
                    })
                    .collect()
            }
        };

        Ok(commitments)
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

        let mut delay: Option<_> = None;

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

                    if let Some(ref mut validator) = validator {
                        log::debug!("Pushing commitments to local validator...");
                        validator.push_commitments(commitments)?;

                        if let Some(ref mut network_sender) = network_sender {
                            log::debug!("Publishing commitments to network...");
                            validator.publish_commitments(network_sender)?;
                        }

                        if let Some(ref mut sequencer) = sequencer {
                            let origin = validator.pub_key().to_address();
                            if validator.has_codes_commit() {
                                let aggregated_codes_commitments = validator.codes_aggregation()?;
                                log::debug!("Received ({}) signed code commitments from local validator...", aggregated_codes_commitments.len());
                                sequencer.receive_codes_commitment(origin, aggregated_codes_commitments)?;
                            }
                            if validator.has_transitions_commit() {
                                let aggregated_transitions_commitments = validator.blocks_aggregation()?;
                                log::debug!("Received ({}) signed transition commitments from local validator...", aggregated_transitions_commitments.len());
                                sequencer.receive_block_commitment(origin, aggregated_transitions_commitments)?;
                            } else {
                                log::debug!("No commitments from local validator...");
                            }
                        }
                    }

                    log::trace!("Sending timeout after observer event...");
                    delay = Some(tokio::time::sleep(block_time / 4));
                }
                _ = maybe_await(delay.take()) => {
                    log::debug!("Sending timeout after block event...");

                    if let Some(sequencer) = sequencer.as_mut() {
                        sequencer.process_block_timeout().await?;
                    }

                    if let Some(ref mut validator) = validator {
                        // clean validator state
                        validator.clear();
                    };
                }
                event = maybe_await(network_receiver.as_mut().map(|rx| rx.recv())) => {
                    if let Some(NetworkReceiverEvent::Commitments { source, data }) = event {
                        if let Some(sequencer) = sequencer.as_mut() {
                            log::debug!("Received p2p commitments from: {:?}", source);

                            let (origin, (codes_aggregated_commitment, transitions_aggregated_commitment)) = Decode::decode(&mut data.as_slice())?;

                            sequencer.receive_codes_commitment(origin, codes_aggregated_commitment)?;
                            sequencer.receive_block_commitment(origin, transitions_aggregated_commitment)?;
                        }
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
            ethereum_rpc: "ws://54.67.75.1:8546".into(),
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
        })
        .await
        .unwrap();
    }
}

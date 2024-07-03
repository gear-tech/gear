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

//! Main service in hypercore node.

use crate::config::{Config, SequencerConfig, ValidatorConfig};
use anyhow::{Ok, Result};
use futures::{future, stream::StreamExt};
use gprimitives::H256;
use hypercore_db::{BlockInfo, BlockMetaInfo, Database};
use hypercore_network::service::NetworkGossip;
use hypercore_processor::{LocalOutcome, TransitionOutcome};
use parity_scale_codec::{Decode, Encode};
use std::sync::Arc;
use tokio::{signal, time};

/// Hypercore service.
pub struct Service {
    db: hypercore_db::Database,
    network: hypercore_network::NetworkWorker,
    observer: hypercore_observer::Observer,
    query: hypercore_observer::Query,
    processor: hypercore_processor::Processor,
    signer: hypercore_signer::Signer,
    sequencer: Option<hypercore_sequencer::Sequencer>,
    validator: Option<hypercore_validator::Validator>,
}

async fn maybe_sleep(maybe_timer: &mut Option<time::Sleep>) {
    if let Some(timer) = maybe_timer.take() {
        timer.await
    } else {
        future::pending().await
    }
}

impl Service {
    pub async fn new(config: &Config) -> Result<Self> {
        let rocks_db = hypercore_db::RocksDatabase::open(config.database_path.clone())?;
        let db = hypercore_db::Database::from_one(&rocks_db);
        let network = hypercore_network::NetworkWorker::new(config.net_config.clone())?;
        let blob_reader = hypercore_observer::ConsensusLayerBlobReader::new(
            &config.ethereum_rpc,
            &config.ethereum_beacon_rpc,
        )
        .await?;
        let ethereum_router_address = config.ethereum_router_address.parse()?;
        let observer = hypercore_observer::Observer::new(
            &config.ethereum_rpc,
            ethereum_router_address,
            Arc::new(blob_reader),
        )
        .await?;
        let query = hypercore_observer::Query::new(
            Box::new(db.clone()),
            &config.ethereum_rpc,
            ethereum_router_address,
        )
        .await?;
        let processor = hypercore_processor::Processor::new(db.clone())?;
        let signer = hypercore_signer::Signer::new(config.key_path.clone())?;

        let sequencer = match config.sequencer {
            SequencerConfig::Enabled(ref sign_tx_public) => Some(
                hypercore_sequencer::Sequencer::new(
                    &hypercore_sequencer::Config {
                        ethereum_rpc: config.ethereum_rpc.clone(),
                        sign_tx_public: *sign_tx_public,
                        router_address: config.ethereum_router_address.parse()?,
                    },
                    signer.clone(),
                )
                .await?,
            ),
            SequencerConfig::Disabled => None,
        };

        let validator = match config.validator {
            ValidatorConfig::Enabled(ref sign_tx_public) => {
                Some(hypercore_validator::Validator::new(
                    &hypercore_validator::Config {
                        pub_key: *sign_tx_public,
                        router_address: config.ethereum_router_address.parse()?,
                    },
                    signer.clone(),
                ))
            }
            ValidatorConfig::Disabled => None,
        };

        Ok(Self {
            db,
            network,
            observer,
            query,
            processor,
            sequencer,
            signer,
            validator,
        })
    }

    async fn sync_block_state(_block_hash: H256) -> Result<()> {
        // TODO: implement
        Ok(())
    }

    async fn process_one_block(
        db: &Database,
        query: &mut hypercore_observer::Query,
        processor: &mut hypercore_processor::Processor,
        block_hash: H256,
    ) -> Result<Vec<LocalOutcome>> {
        if let Some(outcomes_encoded) = db.block_outcome(block_hash) {
            // If outcomes are already processed for the block, just append them.
            let transition_outcomes: Vec<TransitionOutcome> =
                Decode::decode(&mut outcomes_encoded.as_slice())?;
            let block_outcomes: Vec<_> = transition_outcomes
                .into_iter()
                .map(LocalOutcome::Transition)
                .collect();

            return Ok(block_outcomes);
        }

        let parent_hash = query.get_block_parent_hash(block_hash).await?;

        // Check state is valid to continue execution
        if !db.end_state_is_valid(parent_hash).unwrap_or(false) {
            // Sync db state for block
            Self::sync_block_state(block_hash).await?;
            // Set parent block as valid - means state db has all states for the end of parent block
            db.set_end_state_is_valid(parent_hash, true);
        }

        let block_events = query.get_block_events(block_hash).await?;

        query.preset_block_program_hashes(block_hash).await?;

        let block_outcomes = processor.process_block_events(block_hash, &block_events)?;

        let transition_outcomes: Vec<TransitionOutcome> = block_outcomes
            .iter()
            .map(|outcome| {
                let LocalOutcome::Transition(outcome) = outcome else {
                    unreachable!("Only transitions are expected here");
                };
                outcome.clone()
            })
            .collect();

        // TODO: consider
        // if transition_outcomes.is_empty() {
        //     // Empty outcomes case: consider this block as commitment.
        //     db.set_block_has_commitment(block_hash, true);
        // }

        db.set_block_outcome(block_hash, transition_outcomes.encode());

        // Set block as valid - means state db has all states for the end of block
        db.set_end_state_is_valid(block_hash, true);

        Ok(block_outcomes)
    }

    async fn process_block_event(
        db: &Database,
        query: &mut hypercore_observer::Query,
        processor: &mut hypercore_processor::Processor,
        block_data: &hypercore_observer::BlockEventData,
    ) -> Result<Vec<LocalOutcome>> {
        db.set_block_events(block_data.block_hash, block_data.events.encode());
        db.set_parent_hash(block_data.block_hash, block_data.parent_hash);
        db.set_block_info(
            block_data.block_hash,
            BlockInfo {
                height: block_data.block_number.try_into()?,
                timestamp: block_data.block_timestamp,
            },
        );

        let mut outcomes = vec![];
        let commitment_chain = query.get_commitment_chain(block_data.block_hash).await?;
        for block_hash in commitment_chain.into_iter().rev() {
            outcomes.append(&mut Self::process_one_block(db, query, processor, block_hash).await?);
        }

        Ok(outcomes)
    }

    async fn process_observer_event(
        db: &Database,
        query: &mut hypercore_observer::Query,
        processor: &mut hypercore_processor::Processor,
        maybe_sequencer: &mut Option<hypercore_sequencer::Sequencer>,
        observer_event: hypercore_observer::Event,
    ) -> Result<Vec<LocalOutcome>> {
        let outcomes = match &observer_event {
            hypercore_observer::Event::Block(block_data) => {
                Self::process_block_event(db, query, processor, block_data).await?
            }
            hypercore_observer::Event::UploadCode {
                origin: _,
                code_id,
                code,
            } => processor.process_upload_code(*code_id, code.as_slice())?,
        };

        if let Some(sequencer) = maybe_sequencer {
            sequencer.process_observer_event(&observer_event)?;
        }

        Ok(outcomes)
    }

    pub async fn run(self) -> Result<()> {
        let Service {
            db,
            network,
            mut observer,
            mut query,
            mut processor,
            mut sequencer,
            signer: _signer,
            mut validator,
        } = self;

        let network_service = network.service().clone();

        let observer_events = observer.events();
        futures::pin_mut!(observer_events);

        let mut gossip_stream = network_service.gossip_message_stream();

        let network_run = network.run();

        // spawn network future
        let mut network_handle = tokio::spawn(network_run);

        let mut delay: Option<_> = None;

        let mut roles = "Observer".to_string();
        if let Some(seq) = sequencer.as_ref() {
            roles.push_str(&format!(", Sequencer ({})", seq.address()));
        }

        if let Some(val) = validator.as_ref() {
            roles.push_str(&format!(", Validator ({})", val.address()));
        }

        log::info!("⚙️Node service starting, roles: [{}]", roles);

        loop {
            tokio::select! {
                _ = signal::ctrl_c() => {
                    log::info!("Received SIGINT, shutting down...");
                    break;
                }
                observer_event = observer_events.next() => {
                    if let Some(observer_event) = observer_event {
                        let outcomes = Self::process_observer_event(
                            &db,
                            &mut query,
                            &mut processor,
                            &mut sequencer,
                            observer_event,
                        ).await?;

                        if let Some(ref mut validator) = validator {
                            log::debug!("Pushing commitments to local validator...");
                            validator.push_commitment(network_service.clone(), &outcomes)?;

                            if let Some(ref mut sequencer) = sequencer {
                                let origin = validator.pub_key().to_address();

                                if validator.has_codes_commit() {
                                    let aggregated_codes_commitments = validator.codes_aggregation()?;
                                    log::debug!("Received ({}) signed code commitments from local validator...", aggregated_codes_commitments.len());
                                    sequencer.receive_codes_commitment(origin, aggregated_codes_commitments)?;
                                }

                                if validator.has_transitions_commit() {
                                    let aggregated_transitions_commitments = validator.transitions_aggregation()?;
                                    log::debug!("Received ({}) signed transition commitments from local validator...", aggregated_transitions_commitments.len());
                                    sequencer.receive_transitions_commitment(origin, aggregated_transitions_commitments)?;
                                } else {
                                    log::debug!("No commitments from local validator...");
                                }
                            }
                        }

                        delay = Some(tokio::time::sleep(std::time::Duration::from_secs(3)));
                    } else {
                        log::info!("Observer stream ended, shutting down...");
                        break;
                    }
                }
                message = gossip_stream.next() => {
                    if let Some(message) = message {
                        if let Some(sequencer) = sequencer.as_mut() {
                            log::debug!("Received p2p commitments from: {:?}", message.sender);

                            let (origin, (codes_aggregated_commitment, transitions_aggregated_commitment)) = Decode::decode(&mut message.data.as_slice())?;

                            sequencer.receive_codes_commitment(origin, codes_aggregated_commitment)?;
                            sequencer.receive_transitions_commitment(origin, transitions_aggregated_commitment)?;
                        }
                    }
                }
                _ = maybe_sleep(&mut delay) => {
                    log::debug!("Sending timeout after block event...");

                    if let Some(sequencer) = sequencer.as_mut() {
                        sequencer.process_block_timeout().await?;
                    }

                    if let Some(ref mut validator) = validator {
                        // clean validator state
                        validator.clear();
                    };
                }
                _ = &mut network_handle => {
                    log::info!("`NetworkWorker` has terminated, shutting down...");
                    break;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::Service;
    use crate::config::Config;

    #[tokio::test]
    async fn basics() {
        let service = Service::new(&Config {
            database_path: "/tmp/db".into(),
            ethereum_rpc: "wss://ethereum-holesky-rpc.publicnode.com".into(),
            ethereum_beacon_rpc: "http://localhost:5052".into(),
            ethereum_router_address: "0x9F1291e0DE8F29CC7bF16f7a8cb39e7Aebf33B9b".into(),
            ethereum_program_address: "0x23a4FC5f430a7c3736193B852Ad5191c7EC01037".into(),
            key_path: "/tmp/key".into(),
            network_path: "/tmp/net".into(),
            net_config: hypercore_network::NetworkConfiguration::new_local(),
            sequencer: Default::default(),
            validator: Default::default(),
            sender_address: Default::default(),
        })
        .await;

        assert!(service.is_ok());
    }
}

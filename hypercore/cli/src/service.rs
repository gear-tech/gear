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
use anyhow::{anyhow, Ok, Result};
use futures::{future, stream::StreamExt};
use gprimitives::H256;
use hypercore_db::{BlockHeaderMeta, BlockMetaInfo, Database};
use hypercore_network::service::NetworkGossip;
use hypercore_observer::UploadCodeData;
use hypercore_processor::{LocalOutcome, TransitionOutcome};
use hypercore_sequencer::{BlockCommitment, StateTransition};
use hypercore_validator::Commitment;
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
        let blob_reader = Arc::new(
            hypercore_observer::ConsensusLayerBlobReader::new(
                &config.ethereum_rpc,
                &config.ethereum_beacon_rpc,
            )
            .await?,
        );

        let ethereum_router_address = config.ethereum_router_address.parse()?;
        let observer = hypercore_observer::Observer::new(
            &config.ethereum_rpc,
            ethereum_router_address,
            blob_reader.clone(),
        )
        .await?;

        let router_query =
            hypercore_ethereum::RouterQuery::new(&config.ethereum_rpc, ethereum_router_address)
                .await?;
        let genesis_block_hash = router_query.genesis_block_hash().await?;
        log::info!("👶 Genesis block hash: {genesis_block_hash}");

        let query = hypercore_observer::Query::new(
            Box::new(db.clone()),
            &config.ethereum_rpc,
            ethereum_router_address,
            genesis_block_hash,
            blob_reader,
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

    // TODO: remove this function.
    // This is a temporary solution to download absent codes from already processed blocks.
    async fn process_upload_codes(
        db: &Database,
        query: &mut hypercore_observer::Query,
        processor: &mut hypercore_processor::Processor,
        block_hash: H256,
    ) -> Result<()> {
        let (pending_upload_codes, block_events) = query.get_events(block_hash).await?;
        for pending_upload_code in pending_upload_codes {
            let code_id = pending_upload_code.code_id;
            let origin = pending_upload_code.origin;
            db.set_code_upload_info(code_id, (origin, pending_upload_code.blob_tx()));
        }

        for event in block_events.iter() {
            let hypercore_observer::BlockEvent::CreateProgram(event) = event else {
                continue;
            };

            let code_id = event.code_id;
            if db.read_original_code(code_id).is_some() {
                continue;
            }

            log::debug!("📥 downloading absent code: {code_id}");
            let (origin, tx_hash) = db
                .code_upload_info(code_id)
                .ok_or(anyhow!("Origin and tx hash not found"))?;
            let code = query.download_code(code_id, origin, tx_hash).await?;
            processor.process_upload_code(code_id, code.as_slice())?;
        }

        Ok(())
    }

    async fn process_one_block(
        db: &Database,
        query: &mut hypercore_observer::Query,
        processor: &mut hypercore_processor::Processor,
        block_hash: H256,
    ) -> Result<Vec<TransitionOutcome>> {
        if let Some(outcomes_encoded) = db.block_outcome(block_hash) {
            // If outcomes are already processed for the block, just use them.
            let transition_outcomes: Vec<TransitionOutcome> =
                Decode::decode(&mut outcomes_encoded.as_slice())?;
            return Ok(transition_outcomes);
        }

        query.propagate_meta_for_block(block_hash).await?;

        Self::process_upload_codes(db, query, processor, block_hash).await?;

        let block_events = query.get_block_events(block_hash).await?;

        let block_outcomes = processor.process_block_events(block_hash, &block_events)?;

        let transition_outcomes: Vec<TransitionOutcome> = block_outcomes
            .into_iter()
            .map(|outcome| {
                if let LocalOutcome::Transition(outcome) = outcome {
                    outcome
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

        db.set_block_outcome(block_hash, transition_outcomes.encode());

        // Set block as valid - means state db has all states for the end of the block
        db.set_end_state_is_valid(block_hash, true);

        Ok(transition_outcomes)
    }

    async fn process_block_event(
        db: &Database,
        query: &mut hypercore_observer::Query,
        processor: &mut hypercore_processor::Processor,
        block_data: hypercore_observer::BlockEventData,
    ) -> Result<Vec<BlockCommitment>> {
        db.set_block_events(
            block_data.block_hash,
            (block_data.upload_codes, block_data.events).encode(),
        );
        db.set_block_header(
            block_data.block_hash,
            BlockHeaderMeta {
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
            let outcomes = Self::process_one_block(db, query, processor, block_hash).await?;

            if outcomes.is_empty() {
                // Skip empty blocks
                continue;
            }

            let transitions = outcomes
                .into_iter()
                .map(|transition| StateTransition {
                    program_id: transition.program_id,
                    old_state_hash: transition.old_state_hash,
                    new_state_hash: transition.new_state_hash,
                    outgoing_messages: transition.outgoing_messages,
                })
                .collect();

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
        query: &mut hypercore_observer::Query,
        processor: &mut hypercore_processor::Processor,
        maybe_sequencer: &mut Option<hypercore_sequencer::Sequencer>,
        observer_event: hypercore_observer::Event,
    ) -> Result<Vec<Commitment>> {
        if let Some(sequencer) = maybe_sequencer {
            sequencer.process_observer_event(&observer_event)?;
        }

        let commitments = match observer_event {
            hypercore_observer::Event::Block(block_data) => {
                let commitments =
                    Self::process_block_event(db, query, processor, block_data).await?;
                commitments.into_iter().map(Commitment::Block).collect()
            }
            hypercore_observer::Event::UploadCode(UploadCodeData { code_id, code, .. }) => {
                let outcomes = processor.process_upload_code(code_id, code.as_slice())?;
                outcomes
                    .into_iter()
                    .map(|outcome| match outcome {
                        LocalOutcome::CodeApproved(code_id) => {
                            Commitment::Code(hypercore_sequencer::CodeCommitment {
                                code_id,
                                approved: true,
                            })
                        }
                        LocalOutcome::CodeRejected(code_id) => {
                            Commitment::Code(hypercore_sequencer::CodeCommitment {
                                code_id,
                                approved: false,
                            })
                        }
                        _ => unreachable!("Only code outcomes are expected here"),
                    })
                    .collect()
            }
        };

        Ok(commitments)
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

        log::info!("⚙️ Node service starting, roles: [{}]", roles);

        loop {
            tokio::select! {
                _ = signal::ctrl_c() => {
                    log::info!("Received SIGINT, shutting down...");
                    break;
                }
                observer_event = observer_events.next() => {
                    let Some(observer_event) = observer_event else {
                        log::info!("Observer stream ended, shutting down...");
                        break;
                    };

                    let is_block_event = matches!(observer_event, hypercore_observer::Event::Block(_));

                    let commitments = Self::process_observer_event(
                        &db,
                        &mut query,
                        &mut processor,
                        &mut sequencer,
                        observer_event,
                    ).await?;

                    if let Some(ref mut validator) = validator {
                        log::debug!("Pushing commitments to local validator...");
                        validator.push_commitments(network_service.clone(), commitments)?;
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

                    if is_block_event {
                        // After 3 seconds of block event:
                        // - if validator, clean commitments
                        // - if sequencer, send commitments transactions
                        delay = Some(tokio::time::sleep(std::time::Duration::from_secs(3)));
                    }
                }
                message = gossip_stream.next() => {
                    if let Some(message) = message {
                        if let Some(sequencer) = sequencer.as_mut() {
                            log::debug!("Received p2p commitments from: {:?}", message.sender);

                            let (origin, (codes_aggregated_commitment, transitions_aggregated_commitment)) = Decode::decode(&mut message.data.as_slice())?;

                            sequencer.receive_codes_commitment(origin, codes_aggregated_commitment)?;
                            sequencer.receive_block_commitment(origin, transitions_aggregated_commitment)?;
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
            ethereum_router_address: "0x05069E9045Ca0D2B72840c6A21C7bE588E02089A".into(),
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

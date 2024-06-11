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

use std::str::FromStr;

use crate::config::{Config, SequencerConfig, ValidatorConfig};
use anyhow::Result;
use futures::{future, stream::StreamExt};
use hypercore_network::service::NetworkGossip;
use hypercore_processor::LocalOutcome;
use hypercore_sequencer::{AggregatedCommitments, CodeHashCommitment};
use hypercore_signer::{Address, PublicKey};
use parity_scale_codec::Decode;
use tokio::{signal, time};

/// Hypercore service.
pub struct Service {
    db: hypercore_processor::Database,
    network: hypercore_network::NetworkWorker,
    observer: hypercore_observer::Observer,
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
        let db = hypercore_processor::Database::from_one(&rocks_db);
        let network = hypercore_network::NetworkWorker::new(config.net_config.clone())?;
        let observer = hypercore_observer::Observer::new(
            config.ethereum_rpc.clone(),
            config.ethereum_beacon_rpc.clone(),
            config.ethereum_router_address.clone(),
        )
        .await?;
        let processor = hypercore_processor::Processor::new(db.clone());
        let signer = hypercore_signer::Signer::new(config.key_path.clone())?;

        let sequencer = match config.sequencer {
            SequencerConfig::Enabled(ref sign_tx_public) => {
                Some(hypercore_sequencer::Sequencer::new(
                    &hypercore_sequencer::Config {
                        ethereum_rpc: config.ethereum_rpc.clone(),
                        sign_tx_public: sign_tx_public.clone(),
                    },
                    signer.clone(),
                ))
            }
            SequencerConfig::Disabled => None,
        };

        let validator = match config.validator {
            ValidatorConfig::Enabled(ref sign_tx_public) => {
                Some(hypercore_validator::Validator::new(
                    &hypercore_validator::Config {
                        pub_key: PublicKey::from_str(sign_tx_public)?,
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
            processor,
            sequencer,
            signer,
            validator,
        })
    }

    async fn process_observer_event(
        processor: &mut hypercore_processor::Processor,
        maybe_sequencer: &mut Option<hypercore_sequencer::Sequencer>,
        observer_event: &hypercore_observer::Event,
        outcomes: &mut Vec<LocalOutcome>,
    ) -> Result<()> {
        outcomes.extend(
            processor
                .process_observer_event(observer_event)?
                .into_iter(),
        );

        if let Some(sequencer) = maybe_sequencer {
            sequencer.process_observer_event(observer_event)?;
        }

        Ok(())
    }

    pub async fn run(self) -> Result<()> {
        let Service {
            db: _db,
            network,
            mut observer,
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

        loop {
            tokio::select! {
                _ = signal::ctrl_c() => {
                    log::info!("Received SIGINT, shutting down...");
                    break;
                }
                observer_event = observer_events.next() => {
                    let mut outcomes = Vec::new();
                    if let Some(observer_event) = observer_event {
                        Self::process_observer_event(
                            &mut processor,
                            &mut sequencer,
                            &observer_event,
                            &mut outcomes,
                        ).await?;

                        if let Some(ref mut validator) = validator {
                            validator.push_commitment(network_service.clone(), &outcomes)?;

                            if let Some(ref mut sequencer) = sequencer {
                                if validator.has_commit() {
                                    let origin = validator.pub_key().to_address();
                                    let aggregated_commitments = validator.codes_aggregation()?;
                                    sequencer.receive_codes_commitment(origin, aggregated_commitments)?;
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
                            let (origin, aggregated_commitments) = <(Address, AggregatedCommitments<CodeHashCommitment>)>::decode(&mut message.data.as_slice())?;
                            sequencer.receive_codes_commitment(origin, aggregated_commitments)?;
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
        })
        .await;

        assert!(service.is_ok());
    }
}

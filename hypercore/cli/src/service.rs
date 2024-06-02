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

use crate::config::{Config, SequencerConfig};
use anyhow::Result;
use futures::{future, stream::StreamExt};
use std::time::Duration;
use tokio::signal;

/// Hypercore service.
pub struct Service {
    db: Box<dyn hypercore_db::CASDatabase>,
    network: hypercore_network::Network,
    observer: hypercore_observer::Observer,
    processor: hypercore_processor::Processor,
    signer: hypercore_signer::Signer,
    sequencer: Option<hypercore_sequencer::Sequencer>,
}

impl Service {
    pub async fn new(config: &Config) -> Result<Self> {
        let db: Box<dyn hypercore_db::CASDatabase> = Box::new(hypercore_db::RocksDatabase::open(
            config.database_path.clone(),
        )?);
        let network = hypercore_network::Network::start()?;
        let observer = hypercore_observer::Observer::new(
            config.ethereum_rpc.clone(),
            config.ethereum_beacon_rpc.clone(),
            config.ethereum_router_address.clone(),
        )
        .await?;
        let processor = hypercore_processor::Processor::new(db.clone_boxed());
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

        Ok(Self {
            db,
            network,
            observer,
            processor,
            sequencer,
            signer,
        })
    }

    async fn process_observer_event(
        processor: &mut hypercore_processor::Processor,
        maybe_sequencer: &mut Option<hypercore_sequencer::Sequencer>,
        observer_event: &hypercore_observer::Event,
    ) -> Result<()> {
        processor.process_observer_event(observer_event)?;

        if let Some(sequencer) = maybe_sequencer {
            sequencer.process_observer_event(observer_event)?;
        }

        Ok(())
    }

    pub async fn run(self) -> Result<()> {
        let Service {
            db,
            network,
            mut observer,
            mut processor,
            mut sequencer,
            signer,
        } = self;

        let observer_events = observer.events();
        futures::pin_mut!(observer_events);

        loop {
            tokio::select! {
                _ = signal::ctrl_c() => {
                    log::info!("Received SIGINT, shutting down...");
                    break;
                }
                observer_event = observer_events.next() => {
                    if let Some(observer_event) = observer_event {
                        Self::process_observer_event(
                            &mut processor,
                            &mut sequencer,
                            &observer_event
                        ).await?;
                    } else {
                        log::info!("Observer stream ended, shutting down...");
                        break;
                    }
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
            sequencer: Default::default(),
        })
        .await;

        assert!(service.is_ok());
    }
}

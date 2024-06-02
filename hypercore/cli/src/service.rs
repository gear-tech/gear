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

use crate::config::Config;
use anyhow::Result;
use futures::{future, stream::StreamExt};
use hypercore_observer::{
    alloy::{
        primitives::address,
        providers::{ProviderBuilder, RootProvider},
        pubsub::PubSubFrontend,
        rpc::client::WsConnect,
        transports::Transport,
    },
    BlockEvent,
};
use std::time::Duration;
use tokio::signal;

/// Hypercore service.
pub struct Service {
    db: Box<dyn hypercore_db::Database>,
    network: hypercore_network::Network,
    observer: hypercore_observer::Observer<PubSubFrontend, RootProvider<PubSubFrontend>>,
    processor: hypercore_processor::Processor,
}

impl Service {
    pub async fn new(config: &Config) -> Result<Self> {
        let db: Box<dyn hypercore_db::Database> = Box::new(hypercore_db::RocksDatabase::open(
            config.database_path.clone(),
        )?);
        let network = hypercore_network::Network::start()?;
        let ws = WsConnect::new(config.ethereum_rpc.clone());
        let provider = ProviderBuilder::new().on_ws(ws).await?;
        let observer = hypercore_observer::Observer::new(
            provider,
            address!("9F1291e0DE8F29CC7bF16f7a8cb39e7Aebf33B9b"),
        );
        let processor = hypercore_processor::Processor::new(db.clone_boxed());

        Ok(Self {
            db,
            network,
            observer,
            processor,
        })
    }

    pub async fn run(self) -> Result<()> {
        let Service {
            db,
            network,
            mut observer,
            mut processor,
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
                        processor.process_observer_event(observer_event)?
                    } else {
                        log::debug!("[ETH] Observer is down, shutting down...");
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
            key_path: "/tmp/key".into(),
            network_path: "/tmp/net".into(),
        })
        .await;

        assert!(service.is_ok());
    }
}

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
use hypercore_observer::Event;
use std::time::Duration;
use tokio::signal;

/// Hypercore service.
pub struct Service {
    db: Box<dyn hypercore_db::Database>,
    network: hypercore_network::Network,
    observer: hypercore_observer::Observer,
    processor: hypercore_processor::Processor,
}

impl Service {
    pub fn new(config: &Config) -> Result<Self> {
        let db: Box<dyn hypercore_db::Database> = Box::new(hypercore_db::MemDb::new());
        let network = hypercore_network::Network::start()?;
        let observer =
            hypercore_observer::Observer::new(config.ethereum_rpc.clone(), db.clone_boxed())?;
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
            observer,
            mut processor,
        } = self;

        let mut observer_events = observer.listen();

        loop {
            tokio::select! {
                _ = signal::ctrl_c() => {
                    log::info!("Received SIGINT, shutting down...");
                    break;
                }
                (Some(event), ()) = future::join(observer_events.next(), tokio::time::sleep(Duration::from_secs(1))) => {
                    log::debug!("Received [ETH]: {event:?}");


                    match event {
                        Event::NewHead { hash: chain_head, programs, messages } => {
                            processor.run(chain_head, programs, messages)?
                        }
                        Event::NewCode { hash, code } => {
                            // TODO: handle if was set.
                            processor.new_code(hash, code)?;
                        }
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

    #[test]
    fn basics() {
        let service = Service::new(&Config {
            database_path: "/tmp/db".into(),
            ethereum_rpc: "http://localhost:8545".into(),
            key_path: "/tmp/key".into(),
            network_path: "/tmp/net".into(),
        });

        assert!(service.is_ok());
    }
}

// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use async_trait::async_trait;
pub use env::*;
use ethexe_network::NetworkService;
pub use events::*;

mod env;
mod events;

use futures::StreamExt;
use tracing_subscriber::EnvFilter;

pub fn init_logger() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .without_time()
        .try_init();
}

#[async_trait]
pub trait NetworkExt {
    async fn wait_for_gossipsub_subscription(&mut self, topic: String);
}

#[async_trait]
impl NetworkExt for NetworkService {
    async fn wait_for_gossipsub_subscription(&mut self, topic: String) {
        loop {
            match self.select_next_some().await {
                ethexe_network::NetworkEvent::GossipsubPeerSubscribed { topic: t, .. }
                    if t == topic =>
                {
                    break;
                }
                _ => {}
            }
        }
    }
}

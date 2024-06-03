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

//! Sequencer for hypercore.

mod agro;

use anyhow::Result;
use hypercore_observer::Event;
use hypercore_signer::Signer;

pub struct Config {
    pub ethereum_rpc: String,
    pub sign_tx_public: String,
}

pub struct Sequencer {
    signer: Signer,
    ethereum_rpc: String,
}

impl Sequencer {
    pub fn new(config: &Config, signer: Signer) -> Self {
        Self {
            signer,
            ethereum_rpc: config.ethereum_rpc.clone(),
        }
    }

    // This function should never block.
    pub fn process_observer_event(&mut self, event: &Event) -> Result<()> {
        if let Event::Block {
            ref block_hash,
            events: _,
        } = event
        {
            log::debug!("Processing events for {block_hash:?}");
        }

        Ok(())
    }
}

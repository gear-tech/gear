// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

mod actions;
mod cache;
mod coordinator;
mod participant;
mod session;
mod types;

use crate::{
    engine::{
        roast::{
            core::{ParticipantConfig, RoastParticipant},
            storage::coordinator::CoordinatorConfig,
        },
        storage::RoastStore,
    },
    policy::RoastSessionId,
};
use ethexe_common::Address;
use std::collections::{BTreeSet, HashMap};

pub use types::RoastMessage;

/// ROAST Manager handles threshold signing sessions.
#[derive(Debug)]
pub struct RoastManager<DB> {
    /// Active coordinator sessions (when we are leader)
    coordinators:
        HashMap<RoastSessionId, crate::engine::roast::storage::coordinator::RoastCoordinator<DB>>,
    /// Active participant sessions (when we are participant)
    participants: HashMap<RoastSessionId, RoastParticipant>,
    /// Observed session progress for timeouts/failover
    session_progress: HashMap<RoastSessionId, types::SessionProgress>,
    /// Excluded signers per session
    excluded: HashMap<RoastSessionId, BTreeSet<Address>>,
    /// Database
    db: DB,
    /// This validator's address
    self_address: Address,
    /// Configuration
    coordinator_config: CoordinatorConfig,
    participant_config: ParticipantConfig,
}

impl<DB> RoastManager<DB>
where
    DB: RoastStore,
{
    /// Create new ROAST manager.
    pub fn new(db: DB, self_address: Address) -> Self {
        Self {
            coordinators: HashMap::new(),
            participants: HashMap::new(),
            session_progress: HashMap::new(),
            excluded: HashMap::new(),
            db,
            self_address,
            coordinator_config: CoordinatorConfig::default(),
            participant_config: ParticipantConfig { self_address },
        }
    }
}

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

use crate::engine::{dkg::DkgAction, roast::RoastMessage};
use anyhow::Result;
use std::time::Instant;

pub mod dkg;
pub mod prelude;
pub mod roast;
pub mod storage;

/// Abstraction for engine integrations (time + outbound publishing).
pub trait EngineContext {
    /// Returns the current time for timeout calculations.
    fn now(&self) -> Instant;
    /// Publishes a DKG action to the validator network.
    fn publish_dkg_action(&mut self, action: DkgAction) -> Result<()>;
    /// Publishes a ROAST message to the validator network.
    fn publish_roast_message(&mut self, message: RoastMessage) -> Result<()>;
}

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

//! Round1 helpers for DKG protocol.

use super::protocol::DkgProtocol;
use crate::engine::dkg::DkgErrorKind;
use anyhow::{Result, anyhow};
use ethexe_common::{Address, crypto::DkgRound1};

impl DkgProtocol {
    /// Generate and register Round1 package for this participant.
    pub fn generate_round1(&mut self) -> Result<DkgRound1> {
        // Generate local commitment package and temp key, then loop it back.
        let (package, temp_public_key) = self.participant.round1_package()?;
        let message = DkgRound1 {
            session: self.config.session,
            package: package.clone(),
            temp_public_key,
        };

        self.receive_round1(self.config.self_address, message.clone())?;

        Ok(message)
    }

    /// Process received Round1 package.
    pub fn receive_round1(&mut self, sender: Address, message: DkgRound1) -> Result<()> {
        // Reject wrong sessions and unknown participants early.
        if message.session != self.config.session {
            return Err(anyhow::Error::new(DkgErrorKind::SessionIdMismatch));
        }
        let identifier = self
            .identifier_for(sender)
            .ok_or_else(|| anyhow::Error::new(DkgErrorKind::UnknownParticipant))?;
        if self.round1_packages.contains_key(&identifier) {
            // Ignore duplicates from the same participant.
            return Ok(());
        }

        self.dealer
            .receive_round1_package(
                identifier,
                (message.package.clone(), message.temp_public_key),
            )
            .map_err(|err| anyhow!("Round1 package rejected: {err}"))?;

        self.round1_packages.insert(identifier, message);
        Ok(())
    }

    /// Returns true when all round1 packages have been received.
    pub fn is_round1_complete(&self) -> bool {
        self.round1_packages.len() == self.config.participants.len()
    }

    /// Returns the collected round1 packages (unordered).
    pub fn round1_packages(&self) -> Vec<DkgRound1> {
        self.round1_packages.values().cloned().collect()
    }
}

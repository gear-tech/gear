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

//! Round2 helpers for DKG protocol.

use super::protocol::DkgProtocol;
use crate::engine::dkg::DkgErrorKind;
use anyhow::{Result, anyhow};
use ethexe_common::{
    Address,
    crypto::{DkgRound2, DkgRound2Culprits},
};
use std::collections::BTreeMap;

impl DkgProtocol {
    /// Generate Round2 packages for all participants.
    pub fn generate_round2(&mut self) -> Result<DkgRound2> {
        // Round2 requires all round1 packages first.
        if !self.is_round1_complete() {
            return Err(anyhow::Error::new(DkgErrorKind::Round1NotComplete));
        }

        let round1_packages = self
            .round1_packages
            .iter()
            .map(|(id, msg)| (*id, (msg.package.clone(), msg.temp_public_key)))
            .collect::<BTreeMap<_, _>>();

        // Build encrypted packages for each recipient.
        let packages = self.participant.receive_round1_packages(round1_packages)?;

        Ok(DkgRound2 {
            session: self.config.session,
            packages,
        })
    }

    /// Process received Round2 packages.
    pub fn receive_round2(&mut self, sender: Address, message: DkgRound2) -> Result<()> {
        // Reject wrong sessions and unknown participants early.
        if message.session != self.config.session {
            return Err(anyhow::Error::new(DkgErrorKind::SessionIdMismatch));
        }
        let identifier = self
            .identifier_for(sender)
            .ok_or_else(|| anyhow::Error::new(DkgErrorKind::UnknownParticipant))?;
        if self.round2_packages.contains_key(&identifier) {
            // Ignore duplicates from the same participant.
            return Ok(());
        }

        self.dealer
            .receive_round2_packages_encrypted(identifier, message.packages.clone())
            .map_err(|err| anyhow!("Round2 packages rejected: {err}"))?;

        self.round2_packages.insert(identifier, message);
        Ok(())
    }

    /// Returns true when all round2 packages have been received.
    pub fn is_round2_complete(&self) -> bool {
        self.round2_packages.len() == self.config.participants.len()
    }

    /// Process culprits report.
    pub fn receive_round2_culprits(
        &mut self,
        sender: Address,
        message: DkgRound2Culprits,
    ) -> Result<()> {
        // Culprits reports are validated and fed to the dealer.
        if message.session != self.config.session {
            return Err(anyhow::Error::new(DkgErrorKind::SessionIdMismatch));
        }
        let identifier = self
            .identifier_for(sender)
            .ok_or_else(|| anyhow::Error::new(DkgErrorKind::UnknownParticipant))?;

        let temp_secret_key =
            roast_secp256k1_evm::frost::SigningKey::deserialize(&message.temp_secret_key)?;

        self.dealer
            .receive_round2_culprits(identifier, message.culprits.clone(), temp_secret_key)
            .map_err(|err| anyhow!("Round2 culprits rejected: {err}"))?;

        self.round2_culprits.push(message);
        Ok(())
    }

    /// Returns identifiers of participants accused of misbehavior.
    pub fn round2_culprits(&self) -> Vec<ethexe_common::crypto::DkgIdentifier> {
        self.dealer.round2_culprits().collect()
    }

    /// Returns the collected round2 packages (unordered).
    pub fn round2_packages(&self) -> Vec<DkgRound2> {
        self.round2_packages.values().cloned().collect()
    }

    /// Returns the collected round2 culprits reports.
    pub fn round2_culprit_messages(&self) -> Vec<DkgRound2Culprits> {
        self.round2_culprits.clone()
    }
}

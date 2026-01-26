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

//! Complaint/justification handling for DKG protocol.

use super::protocol::DkgProtocol;
use crate::engine::dkg::DkgErrorKind;
use anyhow::{Result, anyhow};
use ethexe_common::crypto::{DkgComplaint, DkgJustification};
use roast_secp256k1_evm::frost::keys::{SecretShare, SigningShare};

impl DkgProtocol {
    /// Records a complaint against an offender for this session.
    pub fn receive_complaint(&mut self, message: DkgComplaint) -> Result<()> {
        if message.session != self.config.session {
            return Err(anyhow::Error::new(DkgErrorKind::SessionIdMismatch));
        }
        if self.identifier_for(message.complainer).is_none() {
            return Err(anyhow::Error::new(DkgErrorKind::UnknownComplainer));
        }
        if self.identifier_for(message.offender).is_none() {
            return Err(anyhow::Error::new(DkgErrorKind::UnknownOffender));
        }
        self.complaints.push(message);
        Ok(())
    }

    /// Records a justification and returns whether it is valid.
    pub fn receive_justification(&mut self, message: DkgJustification) -> Result<bool> {
        if message.session != self.config.session {
            return Err(anyhow::Error::new(DkgErrorKind::SessionIdMismatch));
        }
        if self.identifier_for(message.complainer).is_none() {
            return Err(anyhow::Error::new(DkgErrorKind::UnknownComplainer));
        }
        if self.identifier_for(message.offender).is_none() {
            return Err(anyhow::Error::new(DkgErrorKind::UnknownOffender));
        }
        // Validate the revealed share against the offender's commitment.
        let is_valid = self.verify_justification(&message)?;
        if is_valid {
            self.complaints.retain(|entry| {
                !(entry.complainer == message.complainer && entry.offender == message.offender)
            });
        }
        self.justifications.push(message);
        Ok(is_valid)
    }

    /// Returns all recorded complaints for this session.
    pub fn complaints(&self) -> Vec<DkgComplaint> {
        self.complaints.clone()
    }

    /// Returns all recorded justifications for this session.
    pub fn justifications(&self) -> Vec<DkgJustification> {
        self.justifications.clone()
    }

    /// Verifies a justification share against the offender's commitment.
    fn verify_justification(&self, message: &DkgJustification) -> Result<bool> {
        // Only verify when a matching complaint exists.
        if !self.complaints.iter().any(|complaint| {
            complaint.offender == message.offender && complaint.complainer == message.complainer
        }) {
            return Ok(false);
        }
        let offender_id = self
            .identifier_for(message.offender)
            .ok_or_else(|| anyhow!("Unknown offender"))?;
        let complainer_id = self
            .identifier_for(message.complainer)
            .ok_or_else(|| anyhow!("Unknown complainer"))?;
        let round1 = self
            .round1_packages
            .get(&offender_id)
            .ok_or_else(|| anyhow!("Missing offender commitment"))?;
        let signing_share = SigningShare::deserialize(&message.share)
            .map_err(|err| anyhow!("Failed to deserialize justification share: {err}"))?;
        let secret_share = SecretShare::new(
            complainer_id,
            signing_share,
            round1.package.commitment().clone(),
        );
        Ok(secret_share.verify().is_ok())
    }
}

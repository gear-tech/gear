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

//! Finalization helpers for DKG protocol.

use super::{Group, GroupSerialization, protocol::DkgProtocol};
use crate::engine::dkg::{DkgErrorKind, FinalizeResult};
use anyhow::{Result, anyhow};
use ethexe_common::crypto::{DkgRound2Culprits, DkgVssCommitment};
use roast_secp256k1_evm::{
    error::DkgParticipantError, frost::keys::VerifiableSecretSharingCommitment,
};

impl DkgProtocol {
    /// Attempt to finalize DKG for this participant.
    pub fn finalize(&mut self) -> Result<FinalizeResult> {
        // Finalize only after all round2 packages are present.
        if !self.is_round2_complete() {
            return Err(anyhow::Error::new(DkgErrorKind::Round2NotComplete));
        }

        let round2_packages = self
            .dealer
            .round2_packages_encrypted(self.self_identifier())
            .ok_or_else(|| anyhow::Error::new(DkgErrorKind::MissingRound2PackagesForSelf))?
            .clone();

        // Let the participant decrypt and validate round2 packages.
        match self
            .participant
            .receive_round2_packages_encrypted(round2_packages)
        {
            Ok((key_package, public_key_package)) => {
                // Aggregate commitments into the final VSS commitment.
                let vss_commitment = self.sum_vss_commitments()?;
                Ok(FinalizeResult::Completed {
                    key_package: Box::new(key_package),
                    public_key_package,
                    vss_commitment,
                })
            }
            Err(DkgParticipantError::InvalidSecretShares) => {
                // Report culprits and provide temp secret key for verification.
                let culprits = self.participant.round2_culprits()?;
                let temp_secret_key = self.participant.temp_secret_key().serialize();

                Ok(FinalizeResult::Culprits(DkgRound2Culprits {
                    session: self.config.session,
                    culprits,
                    temp_secret_key,
                }))
            }
            Err(err) => Err(anyhow!("Round2 finalize failed: {err}")),
        }
    }

    /// Sums per-participant VSS commitments into a single commitment.
    fn sum_vss_commitments(&self) -> Result<DkgVssCommitment> {
        // Sum polynomial commitments coefficient-wise.
        let commitments = self
            .round1_packages
            .values()
            .map(|msg| msg.package.commitment().serialize())
            .collect::<Result<Vec<Vec<Vec<u8>>>, _>>()?;

        let mut iter = commitments.into_iter();
        let first = iter.next().ok_or_else(|| anyhow!("No commitments"))?;
        let coeff_len = first.len();

        let mut sums = vec![<Group as roast_secp256k1_evm::frost::Group>::identity(); coeff_len];

        for serialized_commitment in std::iter::once(first).chain(iter) {
            if serialized_commitment.len() != coeff_len {
                return Err(anyhow!("Commitment length mismatch"));
            }
            for (idx, coeff_bytes) in serialized_commitment.into_iter().enumerate() {
                let serialized: GroupSerialization = coeff_bytes
                    .try_into()
                    .map_err(|_| anyhow!("Invalid coefficient length"))?;
                let element =
                    <Group as roast_secp256k1_evm::frost::Group>::deserialize(&serialized)
                        .map_err(|err| anyhow!("Failed to deserialize coefficient: {err}"))?;
                sums[idx] += element;
            }
        }

        let aggregated = sums
            .into_iter()
            .map(|element| {
                let serialized = <Group as roast_secp256k1_evm::frost::Group>::serialize(&element)
                    .map_err(|err| anyhow!("Failed to serialize commitment: {err}"))?;
                Ok(serialized.to_vec())
            })
            .collect::<Result<Vec<Vec<u8>>>>()?;

        VerifiableSecretSharingCommitment::deserialize(aggregated)
            .map_err(|err| anyhow!("Failed to build VSS commitment: {err}"))
    }
}

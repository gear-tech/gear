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

//! DKG Protocol implementation using ROAST/FROST primitives.

use anyhow::{Result, anyhow};
use ethexe_common::{
    Address,
    crypto::{
        DkgComplaint, DkgIdentifier, DkgJustification, DkgKeyPackage, DkgPublicKeyPackage,
        DkgRound1, DkgRound2, DkgRound2Culprits, DkgSessionId, DkgVssCommitment,
    },
};
use rand::rngs::OsRng;
use roast_secp256k1_evm::{
    dkg::{Dealer, Participant},
    error::DkgParticipantError,
    frost::{
        Secp256K1Keccak256,
        keys::{SecretShare, SigningShare, VerifiableSecretSharingCommitment},
    },
};
use std::collections::BTreeMap;

type Ciphersuite = Secp256K1Keccak256;
type Group = <Ciphersuite as roast_secp256k1_evm::frost::Ciphersuite>::Group;
type GroupSerialization = <Group as roast_secp256k1_evm::frost::Group>::Serialization;

/// DKG protocol configuration
#[derive(Debug, Clone)]
pub struct DkgConfig {
    /// Session ID
    pub session: DkgSessionId,
    /// Threshold (t)
    pub threshold: u16,
    /// All participants (sorted)
    pub participants: Vec<Address>,
    /// Address of this participant
    pub self_address: Address,
}

#[derive(Debug)]
pub enum FinalizeResult {
    Completed {
        key_package: Box<DkgKeyPackage>,
        public_key_package: DkgPublicKeyPackage,
        vss_commitment: DkgVssCommitment,
    },
    Culprits(DkgRound2Culprits),
}

/// DKG Protocol handler
#[derive(Debug)]
pub struct DkgProtocol {
    config: DkgConfig,
    identifiers: BTreeMap<Address, DkgIdentifier>,
    addresses: BTreeMap<DkgIdentifier, Address>,
    participant: Participant,
    dealer: Dealer,
    round1_packages: BTreeMap<DkgIdentifier, DkgRound1>,
    round2_packages: BTreeMap<DkgIdentifier, DkgRound2>,
    complaints: Vec<DkgComplaint>,
    justifications: Vec<DkgJustification>,
    round2_culprits: Vec<DkgRound2Culprits>,
}

impl DkgProtocol {
    /// Create a new DKG protocol instance
    pub fn new(mut config: DkgConfig) -> Result<Self> {
        config.participants.sort();
        if !config.participants.contains(&config.self_address) {
            return Err(anyhow!("Self not in participants list"));
        }

        let mut identifiers = BTreeMap::new();
        for address in config.participants.iter() {
            let identifier = DkgIdentifier::derive(address.as_ref())
                .map_err(|_| anyhow!("Failed to derive identifier"))?;
            identifiers.insert(*address, identifier);
        }
        if identifiers.len() != config.participants.len() {
            return Err(anyhow!("Duplicate participants detected"));
        }

        let addresses = identifiers
            .iter()
            .map(|(address, identifier)| (*identifier, *address))
            .collect::<BTreeMap<_, _>>();
        let participants = identifiers.values().copied().collect::<Vec<_>>();
        let mut rng = OsRng;

        let participant = Participant::new(
            identifiers[&config.self_address],
            participants.len() as u16,
            config.threshold,
            &mut rng,
        )?;
        let dealer = Dealer::new(participants.len() as u16, config.threshold, participants)?;

        Ok(Self {
            config,
            identifiers,
            addresses,
            participant,
            dealer,
            round1_packages: BTreeMap::new(),
            round2_packages: BTreeMap::new(),
            complaints: Vec::new(),
            justifications: Vec::new(),
            round2_culprits: Vec::new(),
        })
    }

    pub fn session(&self) -> DkgSessionId {
        self.config.session
    }

    pub fn participants(&self) -> &[Address] {
        &self.config.participants
    }

    pub fn identifier_for(&self, address: Address) -> Option<DkgIdentifier> {
        self.identifiers.get(&address).copied()
    }

    pub fn address_for_identifier(&self, identifier: DkgIdentifier) -> Option<Address> {
        self.addresses.get(&identifier).copied()
    }

    pub fn self_identifier(&self) -> DkgIdentifier {
        self.identifiers[&self.config.self_address]
    }

    pub fn round2_culprits(&self) -> Vec<DkgIdentifier> {
        self.dealer.round2_culprits().collect()
    }

    /// Generate and register Round1 package for this participant.
    pub fn generate_round1(&mut self) -> Result<DkgRound1> {
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
        if message.session != self.config.session {
            return Err(anyhow!("Session ID mismatch"));
        }
        let identifier = self
            .identifier_for(sender)
            .ok_or_else(|| anyhow!("Unknown participant"))?;
        if self.round1_packages.contains_key(&identifier) {
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

    pub fn is_round1_complete(&self) -> bool {
        self.round1_packages.len() == self.config.participants.len()
    }

    /// Generate Round2 packages for all participants.
    pub fn generate_round2(&mut self) -> Result<DkgRound2> {
        if !self.is_round1_complete() {
            return Err(anyhow!("Round1 not complete"));
        }

        let round1_packages = self
            .round1_packages
            .iter()
            .map(|(id, msg)| (*id, (msg.package.clone(), msg.temp_public_key)))
            .collect::<BTreeMap<_, _>>();

        let packages = self.participant.receive_round1_packages(round1_packages)?;

        Ok(DkgRound2 {
            session: self.config.session,
            packages,
        })
    }

    /// Process received Round2 packages.
    pub fn receive_round2(&mut self, sender: Address, message: DkgRound2) -> Result<()> {
        if message.session != self.config.session {
            return Err(anyhow!("Session ID mismatch"));
        }
        let identifier = self
            .identifier_for(sender)
            .ok_or_else(|| anyhow!("Unknown participant"))?;
        if self.round2_packages.contains_key(&identifier) {
            return Ok(());
        }

        self.dealer
            .receive_round2_packages_encrypted(identifier, message.packages.clone())
            .map_err(|err| anyhow!("Round2 packages rejected: {err}"))?;

        self.round2_packages.insert(identifier, message);
        Ok(())
    }

    pub fn is_round2_complete(&self) -> bool {
        self.round2_packages.len() == self.config.participants.len()
    }

    /// Process culprits report.
    pub fn receive_round2_culprits(
        &mut self,
        sender: Address,
        message: DkgRound2Culprits,
    ) -> Result<()> {
        if message.session != self.config.session {
            return Err(anyhow!("Session ID mismatch"));
        }
        let identifier = self
            .identifier_for(sender)
            .ok_or_else(|| anyhow!("Unknown participant"))?;

        let temp_secret_key =
            roast_secp256k1_evm::frost::SigningKey::deserialize(&message.temp_secret_key)?;

        self.dealer
            .receive_round2_culprits(identifier, message.culprits.clone(), temp_secret_key)
            .map_err(|err| anyhow!("Round2 culprits rejected: {err}"))?;

        self.round2_culprits.push(message);
        Ok(())
    }

    pub fn receive_complaint(&mut self, message: DkgComplaint) -> Result<()> {
        if message.session != self.config.session {
            return Err(anyhow!("Session ID mismatch"));
        }
        if self.identifier_for(message.complainer).is_none() {
            return Err(anyhow!("Unknown complainer"));
        }
        if self.identifier_for(message.offender).is_none() {
            return Err(anyhow!("Unknown offender"));
        }
        self.complaints.push(message);
        Ok(())
    }

    pub fn receive_justification(&mut self, message: DkgJustification) -> Result<bool> {
        if message.session != self.config.session {
            return Err(anyhow!("Session ID mismatch"));
        }
        if self.identifier_for(message.complainer).is_none() {
            return Err(anyhow!("Unknown complainer"));
        }
        if self.identifier_for(message.offender).is_none() {
            return Err(anyhow!("Unknown offender"));
        }
        let is_valid = self.verify_justification(&message)?;
        if is_valid {
            self.complaints.retain(|entry| {
                !(entry.complainer == message.complainer && entry.offender == message.offender)
            });
        }
        self.justifications.push(message);
        Ok(is_valid)
    }

    pub fn identifier_map(&self) -> Vec<(Address, DkgIdentifier)> {
        self.identifiers
            .iter()
            .map(|(addr, identifier)| (*addr, *identifier))
            .collect()
    }

    pub fn round1_packages(&self) -> Vec<DkgRound1> {
        self.round1_packages.values().cloned().collect()
    }

    pub fn round2_packages(&self) -> Vec<DkgRound2> {
        self.round2_packages.values().cloned().collect()
    }

    pub fn complaints(&self) -> Vec<DkgComplaint> {
        self.complaints.clone()
    }

    pub fn justifications(&self) -> Vec<DkgJustification> {
        self.justifications.clone()
    }

    pub fn round2_culprit_messages(&self) -> Vec<DkgRound2Culprits> {
        self.round2_culprits.clone()
    }

    /// Attempt to finalize DKG for this participant.
    pub fn finalize(&mut self) -> Result<FinalizeResult> {
        if !self.is_round2_complete() {
            return Err(anyhow!("Round2 not complete"));
        }

        let round2_packages = self
            .dealer
            .round2_packages_encrypted(self.self_identifier())
            .ok_or_else(|| anyhow!("Missing round2 packages for self"))?
            .clone();

        match self
            .participant
            .receive_round2_packages_encrypted(round2_packages)
        {
            Ok((key_package, public_key_package)) => {
                let vss_commitment = self.sum_vss_commitments()?;
                Ok(FinalizeResult::Completed {
                    key_package: Box::new(key_package),
                    public_key_package,
                    vss_commitment,
                })
            }
            Err(DkgParticipantError::InvalidSecretShares) => {
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

    fn sum_vss_commitments(&self) -> Result<DkgVssCommitment> {
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

    fn verify_justification(&self, message: &DkgJustification) -> Result<bool> {
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

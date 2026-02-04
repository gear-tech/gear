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

use crate::{
    engine::prelude::{
        DkgAction, DkgEngine, DkgEngineEvent, RoastEngine, RoastEngineEvent, RoastMessage,
    },
    validator::{sign_dkg_action, sign_roast_message},
};
use anyhow::Result;
use ethexe_common::{
    Address,
    crypto::{DkgPublicKeyPackage, DkgShare, DkgVssCommitment, SignAggregate},
    ecdsa::PrivateKey,
    network::SignedValidatorMessage,
};
use ethexe_db::Database;
use gprimitives::{ActorId, H256};
use gsigner::secp256k1::Signer;
use std::collections::HashMap;

/// Helper to create a test signer
fn create_test_signer(seed: u8) -> (Signer, Address) {
    let signer = Signer::memory();
    let private_key =
        PrivateKey::from_seed([seed; 32]).expect("seed should produce valid private key");
    let pub_key = signer.import(private_key).expect("imported private key");
    let address = pub_key.to_address();
    (signer, address)
}

/// Simulates a network of validators
pub struct ValidatorNetwork {
    validators: HashMap<Address, ValidatorNode>,
    message_queue: Vec<(Address, SignedValidatorMessage)>,
}

struct ValidatorNode {
    dkg_engine: DkgEngine<Database>,
    roast_engine: RoastEngine<Database>,
    signer: Signer,
    pub_key: ethexe_common::ecdsa::PublicKey,
}

impl ValidatorNetwork {
    pub fn new(num_validators: usize) -> Self {
        let mut validators = HashMap::new();

        for i in 0..num_validators {
            let db = Database::memory();
            let (signer, address) = create_test_signer(i as u8 + 1);
            let private_key = PrivateKey::from_seed([i as u8 + 1; 32]).expect("valid private key");
            let pub_key = signer.import(private_key).expect("imported private key");

            let node = ValidatorNode {
                dkg_engine: DkgEngine::new(db.clone(), address),
                roast_engine: RoastEngine::new(db.clone(), address),
                signer,
                pub_key,
            };

            validators.insert(address, node);
        }

        Self {
            validators,
            message_queue: Vec::new(),
        }
    }

    pub fn get_validator_addresses(&self) -> Vec<Address> {
        let mut addresses: Vec<Address> = self.validators.keys().copied().collect();
        addresses.sort();
        addresses
    }

    pub fn coordinator_address(&self) -> Address {
        self.get_validator_addresses()[0]
    }

    pub fn broadcast_message(&mut self, msg: SignedValidatorMessage) {
        for addr in self.validators.keys() {
            self.message_queue.push((*addr, msg.clone()));
        }
    }

    pub fn broadcast_messages<I: IntoIterator<Item = SignedValidatorMessage>>(&mut self, msgs: I) {
        for msg in msgs {
            self.enqueue_message(msg);
        }
    }

    fn enqueue_message(&mut self, msg: SignedValidatorMessage) {
        self.broadcast_message(msg);
    }

    pub fn deliver_messages(&mut self) -> Result<()> {
        let messages = std::mem::take(&mut self.message_queue);

        for (recipient, msg) in messages {
            if let Some(node) = self.validators.get_mut(&recipient) {
                // Process DKG messages
                use ethexe_common::network::VerifiedValidatorMessage;
                let verified = msg.into_verified();

                let outgoing: Vec<SignedValidatorMessage> = match verified {
                    VerifiedValidatorMessage::DkgRound1(m) => {
                        let actions = node.dkg_engine.handle_event(DkgEngineEvent::Round1 {
                            from: m.address(),
                            message: Box::new(m.data().payload.clone()),
                        })?;
                        sign_dkg_actions(&node.signer, node.pub_key, actions)?
                    }
                    VerifiedValidatorMessage::DkgRound2(m) => {
                        let actions = node.dkg_engine.handle_event(DkgEngineEvent::Round2 {
                            from: m.address(),
                            message: m.data().payload.clone(),
                        })?;
                        sign_dkg_actions(&node.signer, node.pub_key, actions)?
                    }
                    VerifiedValidatorMessage::DkgRound2Culprits(m) => {
                        let actions =
                            node.dkg_engine
                                .handle_event(DkgEngineEvent::Round2Culprits {
                                    from: m.address(),
                                    message: m.data().payload.clone(),
                                })?;
                        sign_dkg_actions(&node.signer, node.pub_key, actions)?
                    }
                    VerifiedValidatorMessage::DkgComplaint(m) => {
                        let actions = node.dkg_engine.handle_event(DkgEngineEvent::Complaint {
                            from: m.address(),
                            message: m.data().payload.clone(),
                        })?;
                        sign_dkg_actions(&node.signer, node.pub_key, actions)?
                    }
                    VerifiedValidatorMessage::DkgJustification(m) => {
                        let actions =
                            node.dkg_engine
                                .handle_event(DkgEngineEvent::Justification {
                                    from: m.address(),
                                    message: m.data().payload.clone(),
                                })?;
                        sign_dkg_actions(&node.signer, node.pub_key, actions)?
                    }
                    VerifiedValidatorMessage::SignSessionRequest(m) => {
                        let messages = node.roast_engine.handle_event(
                            RoastEngineEvent::SignSessionRequest {
                                from: m.address(),
                                request: m.data().payload.clone(),
                            },
                        )?;
                        sign_roast_messages(&node.signer, node.pub_key, messages)?
                    }
                    VerifiedValidatorMessage::SignNonceCommit(m) => {
                        let messages =
                            node.roast_engine
                                .handle_event(RoastEngineEvent::NonceCommit {
                                    commit: m.data().payload.clone(),
                                })?;
                        sign_roast_messages(&node.signer, node.pub_key, messages)?
                    }
                    VerifiedValidatorMessage::SignNoncePackage(m) => {
                        let messages =
                            node.roast_engine
                                .handle_event(RoastEngineEvent::NoncePackage {
                                    package: m.data().payload.clone(),
                                })?;
                        sign_roast_messages(&node.signer, node.pub_key, messages)?
                    }
                    VerifiedValidatorMessage::SignShare(m) => {
                        let messages =
                            node.roast_engine
                                .handle_event(RoastEngineEvent::SignShare {
                                    partial: m.data().payload.clone(),
                                })?;
                        sign_roast_messages(&node.signer, node.pub_key, messages)?
                    }
                    VerifiedValidatorMessage::SignCulprits(m) => {
                        node.roast_engine
                            .handle_event(RoastEngineEvent::SignCulprits {
                                culprits: m.data().payload.clone(),
                            })?;
                        vec![]
                    }
                    VerifiedValidatorMessage::SignAggregate(m) => {
                        node.roast_engine
                            .handle_event(RoastEngineEvent::SignAggregate {
                                aggregate: m.data().payload.clone(),
                            })?;
                        vec![]
                    }
                    _ => vec![],
                };

                // Broadcast or route outgoing messages
                for out_msg in outgoing {
                    self.enqueue_message(out_msg);
                }
            }
        }

        Ok(())
    }

    pub fn process_until_idle(&mut self, max_rounds: usize) -> Result<()> {
        for _ in 0..max_rounds {
            if self.message_queue.is_empty() {
                break;
            }
            self.deliver_messages()?;
        }
        Ok(())
    }

    pub fn start_dkg(&mut self, era: u64, threshold: u16) -> Result<Vec<Address>> {
        let validator_addresses = self.get_validator_addresses();
        for addr in validator_addresses.clone() {
            let messages = {
                let node = self.validators.get_mut(&addr).unwrap();
                let actions = node.dkg_engine.handle_event(DkgEngineEvent::Start {
                    era,
                    validators: validator_addresses.clone(),
                    threshold,
                })?;
                sign_dkg_actions(&node.signer, node.pub_key, actions)?
            };
            self.broadcast_messages(messages);
        }

        Ok(validator_addresses)
    }

    pub fn run_dkg(&mut self, era: u64, threshold: u16, max_rounds: usize) -> Result<Vec<Address>> {
        let validator_addresses = self.start_dkg(era, threshold)?;
        self.process_until_idle(max_rounds)?;
        Ok(validator_addresses)
    }

    pub fn start_roast_signing(
        &mut self,
        coordinator_addr: Address,
        msg_hash: H256,
        era: u64,
        tweak_target: ActorId,
        threshold: u16,
        participants: Vec<Address>,
    ) -> Result<()> {
        let messages = {
            let coordinator = self.validators.get_mut(&coordinator_addr).unwrap();
            let messages =
                coordinator
                    .roast_engine
                    .handle_event(RoastEngineEvent::StartSigning {
                        msg_hash,
                        era,
                        tweak_target,
                        threshold,
                        participants,
                    })?;
            sign_roast_messages(&coordinator.signer, coordinator.pub_key, messages)?
        };
        self.broadcast_messages(messages);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn run_roast_signing(
        &mut self,
        coordinator_addr: Address,
        msg_hash: H256,
        era: u64,
        tweak_target: ActorId,
        threshold: u16,
        participants: Vec<Address>,
        max_rounds: usize,
    ) -> Result<SignAggregate> {
        self.start_roast_signing(
            coordinator_addr,
            msg_hash,
            era,
            tweak_target,
            threshold,
            participants.clone(),
        )?;
        self.process_until_idle(max_rounds)?;

        let coordinator = self.validators.get(&coordinator_addr).unwrap();
        coordinator
            .roast_engine
            .get_signature(msg_hash, era)
            .ok_or_else(|| anyhow::anyhow!("Missing aggregate signature"))
    }

    pub fn assert_dkg_completed(&self, era: u64) {
        for (addr, node) in &self.validators {
            assert!(
                node.dkg_engine.is_completed(era),
                "Validator {:?} did not complete DKG",
                addr
            );
            assert!(
                node.dkg_engine.get_public_key_package(era).is_some(),
                "Validator {:?} has no public key package",
                addr
            );
        }
    }

    pub fn public_key_packages(&self, era: u64) -> Vec<DkgPublicKeyPackage> {
        self.validators
            .values()
            .filter_map(|node| node.dkg_engine.get_public_key_package(era))
            .collect()
    }

    pub fn vss_commitment(&self, era: u64) -> Option<DkgVssCommitment> {
        self.validators
            .values()
            .find_map(|node| node.dkg_engine.get_vss_commitment(era))
    }

    pub fn dkg_shares(&self, era: u64) -> Vec<(Address, Option<DkgShare>)> {
        let mut entries: Vec<(Address, Option<DkgShare>)> = self
            .validators
            .iter()
            .map(|(addr, node)| (*addr, node.dkg_engine.get_dkg_share(era)))
            .collect();
        entries.sort_by_key(|(addr, _)| *addr);
        entries
    }

    pub fn signatures_by_validator(
        &self,
        msg_hash: H256,
        era: u64,
    ) -> Vec<(Address, Option<SignAggregate>)> {
        let mut entries: Vec<(Address, Option<SignAggregate>)> = self
            .validators
            .iter()
            .map(|(addr, node)| (*addr, node.roast_engine.get_signature(msg_hash, era)))
            .collect();
        entries.sort_by_key(|(addr, _)| *addr);
        entries
    }

    pub fn cached_signatures_by_validator(
        &self,
        msg_hash: H256,
        era: u64,
        tweak_target: ActorId,
    ) -> Vec<(Address, Option<SignAggregate>)> {
        let mut entries: Vec<(Address, Option<SignAggregate>)> = self
            .validators
            .iter()
            .map(|(addr, node)| {
                (
                    *addr,
                    node.roast_engine
                        .get_cached_signature(msg_hash, era, tweak_target),
                )
            })
            .collect();
        entries.sort_by_key(|(addr, _)| *addr);
        entries
    }
}

fn sign_dkg_actions(
    signer: &Signer,
    pub_key: ethexe_common::ecdsa::PublicKey,
    actions: Vec<DkgAction>,
) -> Result<Vec<SignedValidatorMessage>> {
    let mut signed = Vec::new();
    for action in actions {
        if let Some(msg) = sign_dkg_action(signer, pub_key, action)? {
            signed.push(msg);
        }
    }
    Ok(signed)
}

fn sign_roast_messages(
    signer: &Signer,
    pub_key: ethexe_common::ecdsa::PublicKey,
    messages: Vec<RoastMessage>,
) -> Result<Vec<SignedValidatorMessage>> {
    messages
        .into_iter()
        .map(|msg| sign_roast_message(signer, pub_key, msg))
        .collect()
}

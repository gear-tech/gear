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

use crate::{gossipsub::MessageAcceptance, peer_score};
use anyhow::Context;
use ethexe_common::{
    Address, BlockHeader,
    db::OnChainStorageRead,
    ecdsa::VerifiedData,
    network::{
        SignedValidatorMessage, ValidatorMessage, ValidatorMessagePayload, VerifiedValidatorMessage,
    },
};
use ethexe_db::Database;
use gprimitives::H256;
use libp2p::PeerId;
use nonempty::NonEmpty;
use std::{
    cmp::Ordering,
    collections::{HashMap, VecDeque},
    mem,
};

type InnerValidatorMessage = VerifiedData<ValidatorMessage>;

#[auto_impl::auto_impl(&, Box)]
pub trait ValidatorDatabase: Send + OnChainStorageRead {
    fn clone_boxed(&self) -> Box<dyn ValidatorDatabase>;
}

impl ValidatorDatabase for Database {
    fn clone_boxed(&self) -> Box<dyn ValidatorDatabase> {
        Box::new(self.clone())
    }
}

#[derive(Debug, derive_more::Display)]
enum VerificationError {
    UnknownBlock,
    OldEra,
    NewEra,
    PeerIsNotValidator,
}

pub(crate) struct Validators {
    genesis_timestamp: u64,
    era_duration: u64,

    cached_messages: HashMap<PeerId, InnerValidatorMessage>,
    verified_messages: VecDeque<InnerValidatorMessage>,
    db: Box<dyn ValidatorDatabase>,
    chain_head: Option<(BlockHeader, NonEmpty<Address>)>,
    peer_score: peer_score::Handle,
}

impl Validators {
    pub(crate) fn new(
        genesis_timestamp: u64,
        era_duration: u64,
        db: Box<dyn ValidatorDatabase>,
        peer_score: peer_score::Handle,
    ) -> Self {
        Self {
            genesis_timestamp,
            era_duration,
            cached_messages: HashMap::new(),
            verified_messages: VecDeque::new(),
            db,
            chain_head: None,
            peer_score,
        }
    }

    pub(crate) fn set_chain_head(&mut self, chain_head: H256) -> anyhow::Result<()> {
        let chain_head_header = self
            .db
            .block_header(chain_head)
            .context("chain head not found")?;
        let validators = self
            .db
            .block_validators(chain_head)
            .context("validators not found")?;
        self.chain_head = Some((chain_head_header, validators));

        self.verify_on_new_chain_head();

        Ok(())
    }

    pub(crate) fn next_message(&mut self) -> Option<VerifiedValidatorMessage> {
        let message = self.verified_messages.pop_front()?;
        let (message, pub_key) = message.into_parts();
        let message = unsafe {
            match message.payload {
                ValidatorMessagePayload::ProducerBlock(announce) => {
                    VerifiedValidatorMessage::ProducerBlock(VerifiedData::new(announce, pub_key))
                }
                ValidatorMessagePayload::RequestBatchValidation(request) => {
                    VerifiedValidatorMessage::RequestBatchValidation(VerifiedData::new(
                        request, pub_key,
                    ))
                }
                ValidatorMessagePayload::ApproveBatch(reply) => {
                    VerifiedValidatorMessage::ApproveBatch(VerifiedData::new(reply, pub_key))
                }
            }
        };
        Some(message)
    }

    fn block_era_index(&self, block_ts: u64) -> u64 {
        (block_ts - self.genesis_timestamp) / self.era_duration
    }

    fn inner_verify(&self, message: &InnerValidatorMessage) -> Result<(), VerificationError> {
        let (chain_head, validators) = self
            .chain_head
            .as_ref()
            .expect("chain head should be set by this time");
        let chain_head_era = self.block_era_index(chain_head.timestamp);

        let block = message.data().block;
        let address = message.address();

        let Some(block_header) = self.db.block_header(block) else {
            return Err(VerificationError::UnknownBlock);
        };
        let block_era = self.block_era_index(block_header.timestamp);
        match block_era.cmp(&chain_head_era) {
            Ordering::Less => {
                // node may be not synced yet
                return Err(VerificationError::OldEra);
            }
            Ordering::Equal => {
                // both nodes are in sync
            }
            Ordering::Greater => {
                // node may be synced ahead
                return Err(VerificationError::NewEra);
            }
        }

        if !validators.contains(&address) {
            return Err(VerificationError::PeerIsNotValidator);
        }

        Ok(())
    }

    fn verify_on_new_chain_head(&mut self) {
        let cached_messages = mem::take(&mut self.cached_messages);
        for (source, message) in cached_messages {
            match self.inner_verify(&message) {
                Ok(()) => {
                    self.verified_messages.push_back(message);
                }
                Err(err) => {
                    log::trace!("{message:?} message verification {source} peer failed: {err}");
                    self.peer_score.invalid_data(source);
                }
            }
        }
    }

    pub(crate) fn verify_message_initially(
        &mut self,
        source: PeerId,
        message: SignedValidatorMessage,
    ) -> MessageAcceptance {
        let message = match message.verified() {
            Ok(message) => message,
            Err(error) => {
                log::trace!("failed to validate validator message: {error}");
                self.peer_score.invalid_data(source);
                return MessageAcceptance::Reject;
            }
        };

        match self.inner_verify(&message) {
            Ok(()) => {
                self.verified_messages.push_back(message);
                MessageAcceptance::Accept
            }
            Err(VerificationError::UnknownBlock) => {
                self.cached_messages.insert(source, message);
                MessageAcceptance::Ignore
            }
            Err(VerificationError::OldEra) => MessageAcceptance::Ignore,
            Err(VerificationError::NewEra) => {
                self.cached_messages.insert(source, message);
                MessageAcceptance::Ignore
            }
            Err(VerificationError::PeerIsNotValidator) => {
                log::trace!("peer {source} is not in validator set");
                self.peer_score.invalid_data(source);
                MessageAcceptance::Reject
            }
        }
    }
}

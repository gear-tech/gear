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
use ethexe_common::{
    Address,
    network::{SignedValidatorMessage, VerifiedValidatorMessage},
};
use libp2p::PeerId;
use nonempty::NonEmpty;

#[derive(Debug)]
pub(crate) struct Validators {
    current_validators: Option<NonEmpty<Address>>,
    peer_score: peer_score::Handle,
}

impl Validators {
    pub(crate) fn new(peer_score: peer_score::Handle) -> Self {
        Self {
            current_validators: None,
            peer_score,
        }
    }

    pub(crate) fn set_validators(&mut self, validators: NonEmpty<Address>) {
        self.current_validators = Some(validators);
    }

    pub(crate) fn verify_message(
        &self,
        source: PeerId,
        message: SignedValidatorMessage,
    ) -> (Option<VerifiedValidatorMessage>, MessageAcceptance) {
        let Some(current_validators) = &self.current_validators else {
            return (None, MessageAcceptance::Ignore);
        };

        let message = match message.verified() {
            Ok(message) => message,
            Err(error) => {
                log::trace!("failed to validate validator message: {error}");
                self.peer_score.invalid_data(source);
                return (None, MessageAcceptance::Reject);
            }
        };

        let validator_address = message.address();
        if !current_validators.contains(&validator_address) {
            return (None, MessageAcceptance::Ignore);
        }

        (Some(message), MessageAcceptance::Accept)
    }
}

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

use crate::engine::prelude::{DkgAction, RoastMessage};
use anyhow::Result;
use ethexe_common::{
    ecdsa::PublicKey,
    network::{SignedValidatorMessage, ValidatorMessage},
};
use gsigner::secp256k1::{Secp256k1SignerExt, Signer};

/// Signs outbound DKG actions into validator network messages.
pub(crate) fn sign_dkg_action(
    signer: &Signer,
    pub_key: PublicKey,
    action: DkgAction,
) -> Result<Option<SignedValidatorMessage>> {
    // Wrap action payload with era index and sign it.
    let signed = match action {
        DkgAction::BroadcastRound1(round1) => {
            let message = ValidatorMessage {
                era_index: round1.session.era,
                payload: *round1,
            };
            SignedValidatorMessage::DkgRound1(signer.signed_data(pub_key, message, None)?)
        }
        DkgAction::BroadcastRound2(round2) => {
            let message = ValidatorMessage {
                era_index: round2.session.era,
                payload: round2,
            };
            SignedValidatorMessage::DkgRound2(signer.signed_data(pub_key, message, None)?)
        }
        DkgAction::BroadcastComplaint(complaint) => {
            let message = ValidatorMessage {
                era_index: complaint.session.era,
                payload: complaint,
            };
            SignedValidatorMessage::DkgComplaint(signer.signed_data(pub_key, message, None)?)
        }
        DkgAction::BroadcastRound2Culprits(culprits) => {
            let message = ValidatorMessage {
                era_index: culprits.session.era,
                payload: culprits,
            };
            SignedValidatorMessage::DkgRound2Culprits(signer.signed_data(pub_key, message, None)?)
        }
        DkgAction::Complete(_result) => {
            return Ok(None);
        }
    };

    Ok(Some(signed))
}

/// Signs outbound ROAST messages into validator network messages.
pub(crate) fn sign_roast_message(
    signer: &Signer,
    pub_key: PublicKey,
    msg: RoastMessage,
) -> Result<SignedValidatorMessage> {
    // Wrap message payload with era index and sign it.
    let signed = match msg {
        RoastMessage::SignSessionRequest(request) => {
            let message = ValidatorMessage {
                era_index: request.session.era,
                payload: request,
            };
            SignedValidatorMessage::SignSessionRequest(signer.signed_data(pub_key, message, None)?)
        }
        RoastMessage::SignNonceCommit(commit) => {
            let message = ValidatorMessage {
                era_index: commit.session.era,
                payload: commit,
            };
            SignedValidatorMessage::SignNonceCommit(signer.signed_data(pub_key, message, None)?)
        }
        RoastMessage::SignNoncePackage(package) => {
            let message = ValidatorMessage {
                era_index: package.session.era,
                payload: package,
            };
            SignedValidatorMessage::SignNoncePackage(signer.signed_data(pub_key, message, None)?)
        }
        RoastMessage::SignShare(share) => {
            let message = ValidatorMessage {
                era_index: share.session.era,
                payload: share,
            };
            SignedValidatorMessage::SignShare(signer.signed_data(pub_key, message, None)?)
        }
        RoastMessage::SignAggregate(aggregate) => {
            let message = ValidatorMessage {
                era_index: aggregate.session.era,
                payload: aggregate,
            };
            SignedValidatorMessage::SignAggregate(signer.signed_data(pub_key, message, None)?)
        }
        RoastMessage::SignCulprits(culprits) => {
            let message = ValidatorMessage {
                era_index: culprits.session.era,
                payload: culprits,
            };
            SignedValidatorMessage::SignCulprits(signer.signed_data(pub_key, message, None)?)
        }
    };

    Ok(signed)
}

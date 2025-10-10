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
    Address,
    consensus::{
        SignedAnnounce, SignedValidationReply, SignedValidationRequest, VerifiedAnnounce,
        VerifiedValidationReply, VerifiedValidationRequest,
    },
    ecdsa::SignResult,
};
use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Eq, Encode, Decode, PartialEq, Clone, derive_more::From)]
pub enum SignedValidatorMessage {
    ProducerBlock(SignedAnnounce),
    RequestBatchValidation(SignedValidationRequest),
    ApproveBatch(SignedValidationReply),
}

impl SignedValidatorMessage {
    pub fn verified(self) -> SignResult<VerifiedValidatorMessage> {
        match self {
            SignedValidatorMessage::ProducerBlock(announce) => announce
                .verified()
                .map(VerifiedValidatorMessage::ProducerBlock),
            SignedValidatorMessage::RequestBatchValidation(request) => request
                .verified()
                .map(VerifiedValidatorMessage::RequestBatchValidation),
            SignedValidatorMessage::ApproveBatch(reply) => {
                reply.verified().map(VerifiedValidatorMessage::ApproveBatch)
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum VerifiedValidatorMessage {
    ProducerBlock(VerifiedAnnounce),
    RequestBatchValidation(VerifiedValidationRequest),
    ApproveBatch(VerifiedValidationReply),
}

impl VerifiedValidatorMessage {
    pub fn address(&self) -> Address {
        match self {
            VerifiedValidatorMessage::ProducerBlock(announce) => announce.address(),
            VerifiedValidatorMessage::RequestBatchValidation(request) => request.address(),
            VerifiedValidatorMessage::ApproveBatch(reply) => reply.address(),
        }
    }
}

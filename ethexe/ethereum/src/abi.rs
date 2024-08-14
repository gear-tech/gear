// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use alloy::sol;
use ethexe_common::{mirror, router, wvara};
use gear_core::message::ReplyDetails;
use gear_core_errors::ReplyCode;
use gprimitives::U256;

sol!(
    #[sol(rpc)]
    IMirror,
    "Mirror.json"
);

sol!(
    #[sol(rpc)]
    IMirrorProxy,
    "MirrorProxy.json"
);

sol!(
    #[sol(rpc)]
    IRouter,
    "Router.json"
);

sol!(
    #[sol(rpc)]
    ITransparentUpgradeableProxy,
    "TransparentUpgradeableProxy.json"
);

sol!(
    #[allow(clippy::too_many_arguments)]
    #[sol(rpc)]
    IWrappedVara,
    "WrappedVara.json"
);

/* From common types to alloy */

impl From<router::CodeCommitment> for IRouter::CodeCommitment {
    fn from(router::CodeCommitment { id, valid }: router::CodeCommitment) -> Self {
        Self {
            id: id.into_bytes().into(),
            valid,
        }
    }
}

impl From<router::BlockCommitment> for IRouter::BlockCommitment {
    fn from(
        router::BlockCommitment {
            block_hash,
            prev_commitment_hash,
            pred_block_hash,
            transitions,
        }: router::BlockCommitment,
    ) -> Self {
        Self {
            blockHash: block_hash.to_fixed_bytes().into(),
            prevCommitmentHash: prev_commitment_hash.to_fixed_bytes().into(),
            predBlockHash: pred_block_hash.to_fixed_bytes().into(),
            transitions: transitions.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<router::StateTransition> for IRouter::StateTransition {
    fn from(
        router::StateTransition {
            actor_id,
            new_state_hash,
            value_to_receive,
            value_claims,
            messages,
        }: router::StateTransition,
    ) -> Self {
        Self {
            actorId: actor_id.to_address_lossy().to_fixed_bytes().into(),
            newStateHash: new_state_hash.to_fixed_bytes().into(),
            valueToReceive: value_to_receive,
            valueClaims: value_claims.into_iter().map(Into::into).collect(),
            messages: messages.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<router::ValueClaim> for IRouter::ValueClaim {
    fn from(
        router::ValueClaim {
            message_id,
            destination,
            value,
        }: router::ValueClaim,
    ) -> Self {
        Self {
            messageId: message_id.into_bytes().into(),
            destination: destination.to_address_lossy().to_fixed_bytes().into(),
            value,
        }
    }
}

impl From<router::OutgoingMessage> for IRouter::OutgoingMessage {
    fn from(
        router::OutgoingMessage {
            id,
            destination,
            payload,
            value,
            reply_details,
        }: router::OutgoingMessage,
    ) -> Self {
        Self {
            id: id.into_bytes().into(),
            destination: destination.to_address_lossy().to_fixed_bytes().into(),
            payload: payload.into(),
            value,
            replyDetails: reply_details.into(),
        }
    }
}

impl From<ReplyDetails> for IRouter::ReplyDetails {
    fn from(value: ReplyDetails) -> Self {
        let (to, code) = value.into_parts();

        Self {
            to: to.into_bytes().into(),
            code: code.to_bytes().into(),
        }
    }
}

impl From<Option<ReplyDetails>> for IRouter::ReplyDetails {
    fn from(value: Option<ReplyDetails>) -> Self {
        value.unwrap_or_default().into()
    }
}

/* From alloy types to common */

impl From<IRouter::BaseWeightChanged> for router::Event {
    fn from(event: IRouter::BaseWeightChanged) -> Self {
        router::Event::BaseWeightChanged {
            base_weight: event.baseWeight,
        }
    }
}

impl From<IRouter::BlockCommitted> for router::Event {
    fn from(event: IRouter::BlockCommitted) -> Self {
        router::Event::BlockCommitted {
            block_hash: (*event.blockHash).into(),
        }
    }
}

impl From<IRouter::CodeGotValidated> for router::Event {
    fn from(event: IRouter::CodeGotValidated) -> Self {
        router::Event::CodeGotValidated {
            id: (*event.id).into(),
            valid: event.valid,
        }
    }
}

impl From<IRouter::CodeValidationRequested> for router::Event {
    fn from(event: IRouter::CodeValidationRequested) -> Self {
        router::Event::CodeValidationRequested {
            code_id: (*event.codeId).into(),
            blob_tx_hash: (*event.blobTxHash).into(),
        }
    }
}

impl From<IRouter::ProgramCreated> for router::Event {
    fn from(event: IRouter::ProgramCreated) -> Self {
        router::Event::ProgramCreated {
            actor_id: (*event.actorId.into_word()).into(),
            code_id: (*event.codeId).into(),
        }
    }
}

impl From<IRouter::StorageSlotChanged> for router::Event {
    fn from(_: IRouter::StorageSlotChanged) -> Self {
        router::Event::StorageSlotChanged
    }
}

impl From<IRouter::ValidatorsSetChanged> for router::Event {
    fn from(_: IRouter::ValidatorsSetChanged) -> Self {
        router::Event::ValidatorsSetChanged
    }
}

impl From<IRouter::ValuePerWeightChanged> for router::Event {
    fn from(event: IRouter::ValuePerWeightChanged) -> Self {
        router::Event::ValuePerWeightChanged {
            value_per_weight: event.valuePerWeight,
        }
    }
}

impl From<IMirror::ExecutableBalanceTopUpRequested> for mirror::Event {
    fn from(event: IMirror::ExecutableBalanceTopUpRequested) -> Self {
        mirror::Event::ExecutableBalanceTopUpRequested { value: event.value }
    }
}

impl From<IMirror::Message> for mirror::Event {
    fn from(event: IMirror::Message) -> Self {
        mirror::Event::Message {
            id: (*event.id).into(),
            destination: (*event.destination.into_word()).into(),
            payload: event.payload.to_vec(),
            value: event.value,
        }
    }
}

impl From<IMirror::MessageQueueingRequested> for mirror::Event {
    fn from(event: IMirror::MessageQueueingRequested) -> Self {
        mirror::Event::MessageQueueingRequested {
            id: (*event.id).into(),
            source: (*event.source.into_word()).into(),
            payload: event.payload.to_vec(),
            value: event.value,
        }
    }
}

impl From<IMirror::Reply> for mirror::Event {
    fn from(event: IMirror::Reply) -> Self {
        mirror::Event::Reply {
            payload: event.payload.to_vec(),
            value: event.value,
            reply_to: (*event.replyTo).into(),
            reply_code: ReplyCode::from_bytes(*event.replyCode),
        }
    }
}

impl From<IMirror::ReplyQueueingRequested> for mirror::Event {
    fn from(event: IMirror::ReplyQueueingRequested) -> Self {
        mirror::Event::ReplyQueueingRequested {
            replied_to: (*event.repliedTo).into(),
            source: (*event.source.into_word()).into(),
            payload: event.payload.to_vec(),
            value: event.value,
        }
    }
}

impl From<IMirror::StateChanged> for mirror::Event {
    fn from(event: IMirror::StateChanged) -> Self {
        mirror::Event::StateChanged {
            state_hash: (*event.stateHash).into(),
        }
    }
}

impl From<IMirror::ValueClaimed> for mirror::Event {
    fn from(event: IMirror::ValueClaimed) -> Self {
        mirror::Event::ValueClaimed {
            claimed_id: (*event.claimedId).into(),
            value: event.value,
        }
    }
}

impl From<IMirror::ValueClaimingRequested> for mirror::Event {
    fn from(event: IMirror::ValueClaimingRequested) -> Self {
        mirror::Event::ValueClaimingRequested {
            claimed_id: (*event.claimedId).into(),
            source: (*event.source.into_word()).into(),
        }
    }
}

impl From<IWrappedVara::Transfer> for wvara::Event {
    fn from(event: IWrappedVara::Transfer) -> Self {
        let [.., high, low] = event.value.into_limbs();
        let value = ((high as u128) << 64) | (low as u128);

        wvara::Event::Transfer {
            from: (*event.from.into_word()).into(),
            to: (*event.to.into_word()).into(),
            value,
        }
    }
}

impl From<IWrappedVara::Approval> for wvara::Event {
    fn from(event: IWrappedVara::Approval) -> Self {
        wvara::Event::Approval {
            owner: (*event.owner.into_word()).into(),
            spender: (*event.spender.into_word()).into(),
            value: U256(event.value.into_limbs()),
        }
    }
}

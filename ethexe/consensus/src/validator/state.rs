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

use super::{
    BatchCommitmentValidationReply, ConsensusEvent, ValidatorContext, coordinator::Coordinator,
    dispatcher, initial::Initial, participant::Participant, producer::Producer,
    subordinate::Subordinate,
};
use anyhow::Result;
use derive_more::{Debug, From};
use ethexe_common::{
    ComputedAnnounce, SimpleBlockData,
    consensus::{VerifiedAnnounce, VerifiedValidationRequest},
    injected::SignedInjectedTransaction,
    network::{CheckedAnnouncesResponse, SignedValidatorMessage, VerifiedValidatorMessage},
};
use gprimitives::H256;
use std::{
    fmt,
    task::{Context, Poll},
};

/// An event that can be saved for later processing.
#[derive(Clone, Debug, From, PartialEq, Eq, derive_more::IsVariant)]
pub(crate) enum PendingEvent {
    /// A block from the producer
    Announce(VerifiedAnnounce),
    /// A validation request
    ValidationRequest(VerifiedValidationRequest),
}

/// Trait defining the interface for validator inner state and events handler.
pub(crate) trait StateHandler
where
    Self: Sized + Into<ValidatorState> + fmt::Display,
{
    fn context(&self) -> &ValidatorContext;

    fn context_mut(&mut self) -> &mut ValidatorContext;

    fn into_context(self) -> ValidatorContext;

    fn warning(&mut self, warning: impl fmt::Display) {
        let warning = format!("{self} - {warning}");
        self.context_mut()
            .output
            .push_back(ConsensusEvent::Warning(warning));
    }

    fn process_new_head(self, block: SimpleBlockData) -> Result<ValidatorState> {
        DefaultProcessing::new_head(self.into(), block)
    }

    fn process_synced_block(self, block: H256) -> Result<ValidatorState> {
        DefaultProcessing::synced_block(self.into(), block)
    }

    fn process_prepared_block(self, block: H256) -> Result<ValidatorState> {
        DefaultProcessing::prepared_block(self.into(), block)
    }

    fn process_computed_announce(self, computed_data: ComputedAnnounce) -> Result<ValidatorState> {
        DefaultProcessing::computed_announce(self.into(), computed_data)
    }

    fn process_announce(self, block: VerifiedAnnounce) -> Result<ValidatorState> {
        DefaultProcessing::block_from_producer(self, block)
    }

    fn process_validation_request(
        self,
        request: VerifiedValidationRequest,
    ) -> Result<ValidatorState> {
        DefaultProcessing::validation_request(self, request)
    }

    fn process_validation_reply(
        self,
        reply: BatchCommitmentValidationReply,
    ) -> Result<ValidatorState> {
        DefaultProcessing::validation_reply(self, reply)
    }

    fn process_verified_validator_message(
        self,
        message: ethexe_common::network::VerifiedValidatorMessage,
    ) -> Result<ValidatorState> {
        // Route DKG/ROAST traffic through the dispatcher.
        DefaultProcessing::verified_validator_message(self.into(), message)
    }

    fn process_announces_response(
        self,
        _response: CheckedAnnouncesResponse,
    ) -> Result<ValidatorState> {
        DefaultProcessing::announces_response(self, _response)
    }

    fn process_injected_transaction(self, tx: SignedInjectedTransaction) -> Result<ValidatorState> {
        DefaultProcessing::injected_transaction(self, tx)
    }

    fn process_validator_message(self, message: SignedValidatorMessage) -> Result<ValidatorState> {
        DefaultProcessing::validator_message(self, message)
    }

    fn poll_next_state(self, _cx: &mut Context<'_>) -> Result<(Poll<()>, ValidatorState)> {
        Ok((Poll::Pending, self.into()))
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(
    Debug, derive_more::Display, derive_more::From, derive_more::IsVariant, derive_more::Unwrap,
)]
pub(crate) enum ValidatorState {
    Initial(Initial),
    Producer(Producer),
    Coordinator(Coordinator),
    Subordinate(Subordinate),
    Participant(Participant),
}

macro_rules! delegate_call {
    ($this:ident => $func:ident( $( $arg:ident ),* )) => {
        match $this {
            ValidatorState::Initial(initial) => initial.$func($( $arg ),*),
            ValidatorState::Producer(producer) => producer.$func($( $arg ),*),
            ValidatorState::Coordinator(coordinator) => coordinator.$func($( $arg ),*),
            ValidatorState::Subordinate(subordinate) => subordinate.$func($( $arg ),*),
            ValidatorState::Participant(participant) => participant.$func($( $arg ),*),
        }
    };
}

impl StateHandler for ValidatorState {
    fn context(&self) -> &ValidatorContext {
        delegate_call!(self => context())
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        delegate_call!(self => context_mut())
    }

    fn into_context(self) -> ValidatorContext {
        delegate_call!(self => into_context())
    }

    fn warning(&mut self, warning: impl fmt::Display) {
        delegate_call!(self => warning(warning))
    }

    fn process_new_head(self, block: SimpleBlockData) -> Result<ValidatorState> {
        delegate_call!(self => process_new_head(block))
    }

    fn process_synced_block(self, block: H256) -> Result<ValidatorState> {
        delegate_call!(self => process_synced_block(block))
    }

    fn process_prepared_block(self, block: H256) -> Result<ValidatorState> {
        delegate_call!(self => process_prepared_block(block))
    }

    fn process_computed_announce(self, computed_data: ComputedAnnounce) -> Result<ValidatorState> {
        delegate_call!(self => process_computed_announce(computed_data))
    }

    fn process_announce(self, announce: VerifiedAnnounce) -> Result<ValidatorState> {
        delegate_call!(self => process_announce(announce))
    }

    fn process_validation_request(
        self,
        request: VerifiedValidationRequest,
    ) -> Result<ValidatorState> {
        delegate_call!(self => process_validation_request(request))
    }

    fn process_validation_reply(
        self,
        reply: BatchCommitmentValidationReply,
    ) -> Result<ValidatorState> {
        delegate_call!(self => process_validation_reply(reply))
    }

    fn process_announces_response(
        self,
        response: CheckedAnnouncesResponse,
    ) -> Result<ValidatorState> {
        delegate_call!(self => process_announces_response(response))
    }

    fn process_verified_validator_message(
        self,
        message: ethexe_common::network::VerifiedValidatorMessage,
    ) -> Result<ValidatorState> {
        delegate_call!(self => process_verified_validator_message(message))
    }

    fn poll_next_state(self, cx: &mut Context<'_>) -> Result<(Poll<()>, ValidatorState)> {
        delegate_call!(self => poll_next_state(cx))
    }

    fn process_injected_transaction(self, tx: SignedInjectedTransaction) -> Result<ValidatorState> {
        delegate_call!(self => process_injected_transaction(tx))
    }

    fn process_validator_message(self, message: SignedValidatorMessage) -> Result<ValidatorState> {
        delegate_call!(self => process_validator_message(message))
    }
}

pub(crate) struct DefaultProcessing;

impl DefaultProcessing {
    pub(crate) fn new_head(
        s: impl Into<ValidatorState>,
        block: SimpleBlockData,
    ) -> Result<ValidatorState> {
        // New head always resets to Initial state.
        Initial::create_with_chain_head(s.into().into_context(), block)
    }

    pub(crate) fn synced_block(
        s: impl Into<ValidatorState>,
        block: H256,
    ) -> Result<ValidatorState> {
        let mut s = s.into();
        s.warning(format!("unexpected synced block: {block}"));
        Ok(s)
    }

    pub(crate) fn prepared_block(
        s: impl Into<ValidatorState>,
        block: H256,
    ) -> Result<ValidatorState> {
        let mut s = s.into();
        s.warning(format!("unexpected processed block: {block}"));
        Ok(s)
    }

    pub(crate) fn computed_announce(
        s: impl Into<ValidatorState>,
        computed_data: ComputedAnnounce,
    ) -> Result<ValidatorState> {
        let mut s = s.into();
        s.warning(format!(
            "unexpected computed announce: {}",
            computed_data.announce_hash
        ));
        Ok(s)
    }

    pub(crate) fn block_from_producer(
        s: impl Into<ValidatorState>,
        announce: VerifiedAnnounce,
    ) -> Result<ValidatorState> {
        // Store producer announce until a compatible state consumes it.
        let mut s = s.into();
        s.warning(format!(
            "unexpected block from producer: {announce:?}, saved for later."
        ));
        s.context_mut().pending(announce);
        Ok(s)
    }

    pub(crate) fn validation_request(
        s: impl Into<ValidatorState>,
        request: VerifiedValidationRequest,
    ) -> Result<ValidatorState> {
        // Store validation request until a compatible state consumes it.
        let mut s = s.into();
        s.warning(format!(
            "unexpected validation request: {request:?}, saved for later."
        ));
        s.context_mut().pending(request);
        Ok(s)
    }

    pub(crate) fn validation_reply(
        s: impl Into<ValidatorState>,
        reply: BatchCommitmentValidationReply,
    ) -> Result<ValidatorState> {
        tracing::trace!("Skip validation reply: {reply:?}");
        Ok(s.into())
    }

    pub(crate) fn announces_response(
        s: impl Into<ValidatorState>,
        response: CheckedAnnouncesResponse,
    ) -> Result<ValidatorState> {
        let mut s = s.into();
        s.warning(format!(
            "unexpected announces response: {response:?}, ignored."
        ));
        Ok(s)
    }

    pub(crate) fn injected_transaction(
        s: impl Into<ValidatorState>,
        tx: SignedInjectedTransaction,
    ) -> Result<ValidatorState> {
        let mut s = s.into();
        s.context_mut().core.process_injected_transaction(tx)?;
        Ok(s)
    }

    pub(crate) fn validator_message(
        s: impl Into<ValidatorState>,
        message: SignedValidatorMessage,
    ) -> Result<ValidatorState> {
        // Verify signatures before dispatching.
        Self::verified_validator_message(s, message.into_verified())
    }

    pub(crate) fn verified_validator_message(
        s: impl Into<ValidatorState>,
        message: VerifiedValidatorMessage,
    ) -> Result<ValidatorState> {
        dispatcher::handle_verified_validator_message(s.into(), message)
    }
}

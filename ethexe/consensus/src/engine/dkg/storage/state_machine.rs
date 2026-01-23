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

//! DKG State Machine

use crate::engine::dkg::{
    DkgCompleted, DkgErrorKind, DkgResult, SessionConfig,
    core::{DkgConfig, DkgProtocol, FinalizeResult},
};
use anyhow::{Result, anyhow};
use ethexe_common::{
    Address,
    crypto::{
        DkgComplaint, DkgJustification, DkgKeyPackage, DkgRound1, DkgRound2, DkgRound2Culprits,
        DkgSessionId, DkgShare,
    },
    db::DkgSessionState,
};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

/// DKG state machine states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DkgState {
    /// Idle - no active DKG session
    Idle,
    /// Round1Pending - waiting for round1 packages from all participants
    Round1Pending { started_at: Instant },
    /// Round2Pending - waiting for round2 packages from all participants
    Round2Pending { started_at: Instant },
    /// CulpritsPending - waiting for round2 culprits processing
    CulpritsPending { started_at: Instant },
    /// Completed - DKG finished successfully
    Completed,
    /// Failed - DKG failed
    Failed(String),
}

/// Events that can be processed by the state machine.
#[derive(Debug, Clone)]
pub enum DkgEvent {
    /// Start a new DKG session
    Start(SessionConfig),
    /// Received Round1 package
    Round1 {
        from: Address,
        message: Box<DkgRound1>,
    },
    /// Received Round2 packages
    Round2 {
        from: Address,
        message: Box<DkgRound2>,
    },
    /// Received complaint
    Complaint {
        from: Address,
        message: DkgComplaint,
    },
    /// Received justification
    Justification {
        from: Address,
        message: DkgJustification,
    },
    /// Received Round2 culprits report
    Round2Culprits {
        from: Address,
        message: DkgRound2Culprits,
    },
    /// Timeout occurred
    Timeout,
}

/// Actions to be performed after state transition.
#[derive(Debug, Clone)]
pub enum DkgAction {
    /// Broadcast Round1 package
    BroadcastRound1(Box<DkgRound1>),
    /// Broadcast Round2 packages
    BroadcastRound2(DkgRound2),
    /// Broadcast complaint
    BroadcastComplaint(DkgComplaint),
    /// Broadcast round2 culprits
    BroadcastRound2Culprits(DkgRound2Culprits),
    /// DKG completed with result
    Complete(Box<DkgResult>),
}

/// DKG State Machine.
#[derive(Debug)]
pub struct DkgStateMachine {
    state: DkgState,
    protocol: Option<DkgProtocol>,
    config: Option<SessionConfig>,
    excluded: BTreeSet<Address>,

    // Timeouts
    round1_timeout: Duration,
    round2_timeout: Duration,
    culprits_timeout: Duration,
}

impl Default for DkgStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl DkgStateMachine {
    /// Create new DKG state machine.
    pub fn new() -> Self {
        Self {
            state: DkgState::Idle,
            protocol: None,
            config: None,
            excluded: BTreeSet::new(),
            round1_timeout: Duration::from_secs(30),
            round2_timeout: Duration::from_secs(30),
            culprits_timeout: Duration::from_secs(20),
        }
    }

    /// Get current state.
    pub fn state(&self) -> &DkgState {
        &self.state
    }

    /// Process an event and return actions to perform.
    pub fn process_event(&mut self, event: DkgEvent) -> Result<Vec<DkgAction>> {
        match event {
            DkgEvent::Start(config) => self.handle_start(config),
            DkgEvent::Round1 { from, message } => {
                if matches!(self.state, DkgState::Round1Pending { .. }) {
                    self.handle_round1(from, *message)
                } else {
                    Ok(vec![])
                }
            }
            DkgEvent::Round2 { from, message } => {
                if matches!(self.state, DkgState::Round2Pending { .. }) {
                    self.handle_round2(from, *message)
                } else {
                    Ok(vec![])
                }
            }
            DkgEvent::Complaint { from, message } => self.handle_complaint(from, message),
            DkgEvent::Justification { from, message } => self.handle_justification(from, message),
            DkgEvent::Round2Culprits { from, message } => {
                if matches!(
                    self.state,
                    DkgState::Round2Pending { .. } | DkgState::CulpritsPending { .. }
                ) {
                    self.handle_round2_culprits(from, message)
                } else {
                    Ok(vec![])
                }
            }
            DkgEvent::Timeout => self.handle_timeout(),
        }
    }

    /// Initializes a new DKG session and emits the first round.
    fn handle_start(&mut self, config: SessionConfig) -> Result<Vec<DkgAction>> {
        if !matches!(self.state, DkgState::Idle) {
            return Err(anyhow::Error::new(DkgErrorKind::AlreadyInProgress));
        }

        let protocol_config = DkgConfig {
            session: DkgSessionId {
                era: config.era_index,
            },
            threshold: config.threshold,
            participants: config.validators.clone(),
            self_address: config.self_address,
        };

        // Build the protocol instance and generate our round1 package.
        let mut protocol = DkgProtocol::new(protocol_config)?;
        let round1 = protocol.generate_round1()?;

        self.protocol = Some(protocol);
        self.config = Some(config);
        self.excluded.clear();
        // Move into round1 collection phase.
        self.state = DkgState::Round1Pending {
            started_at: Instant::now(),
        };

        Ok(vec![DkgAction::BroadcastRound1(Box::new(round1))])
    }

    /// Handles a round1 package and emits round2 when complete.
    fn handle_round1(&mut self, from: Address, message: DkgRound1) -> Result<Vec<DkgAction>> {
        // Track round1 packages until all participants are collected.
        let is_complete = {
            let protocol = self
                .protocol
                .as_mut()
                .ok_or_else(|| anyhow::Error::new(DkgErrorKind::NoActiveProtocol))?;
            protocol.receive_round1(from, message)?;
            protocol.is_round1_complete()
        };
        if is_complete {
            // Generate round2 packages once round1 is complete.
            let round2 = self
                .protocol
                .as_mut()
                .ok_or_else(|| anyhow::Error::new(DkgErrorKind::NoActiveProtocol))?
                .generate_round2()?;
            self.state = DkgState::Round2Pending {
                started_at: Instant::now(),
            };
            return Ok(vec![DkgAction::BroadcastRound2(round2)]);
        }

        Ok(vec![])
    }

    /// Handles a round2 package and attempts finalize when complete.
    fn handle_round2(&mut self, from: Address, message: DkgRound2) -> Result<Vec<DkgAction>> {
        // Track round2 packages until all participants are collected.
        let is_complete = {
            let protocol = self
                .protocol
                .as_mut()
                .ok_or_else(|| anyhow::Error::new(DkgErrorKind::NoActiveProtocol))?;
            protocol.receive_round2(from, message)?;
            protocol.is_round2_complete()
        };
        if is_complete {
            return self.try_finalize();
        }

        Ok(vec![])
    }

    /// Handles a complaint message for the current session.
    fn handle_complaint(&mut self, from: Address, message: DkgComplaint) -> Result<Vec<DkgAction>> {
        if message.complainer != from {
            return Ok(vec![]);
        }
        let protocol = self
            .protocol
            .as_mut()
            .ok_or_else(|| anyhow::Error::new(DkgErrorKind::NoActiveProtocol))?;
        protocol.receive_complaint(message)?;
        Ok(vec![])
    }

    /// Handles a justification message and updates complaint state.
    fn handle_justification(
        &mut self,
        from: Address,
        message: DkgJustification,
    ) -> Result<Vec<DkgAction>> {
        if message.offender != from {
            return Ok(vec![]);
        }
        let offender = message.offender;
        let protocol = self
            .protocol
            .as_mut()
            .ok_or_else(|| anyhow::Error::new(DkgErrorKind::NoActiveProtocol))?;
        let is_valid = protocol.receive_justification(message)?;
        if is_valid {
            Ok(vec![])
        } else {
            self.exclude_and_restart(vec![offender])
        }
    }

    /// Handles culprits report and triggers exclusion/restart when needed.
    fn handle_round2_culprits(
        &mut self,
        from: Address,
        message: DkgRound2Culprits,
    ) -> Result<Vec<DkgAction>> {
        let culprit_addresses = {
            let protocol = self
                .protocol
                .as_mut()
                .ok_or_else(|| anyhow::Error::new(DkgErrorKind::NoActiveProtocol))?;

            protocol.receive_round2_culprits(from, message)?;

            protocol
                .round2_culprits()
                .into_iter()
                .filter_map(|culprit| protocol.address_for_identifier(culprit))
                .collect::<Vec<_>>()
        };

        let actions = self.exclude_and_restart(culprit_addresses)?;
        if actions.is_empty() {
            self.state = DkgState::CulpritsPending {
                started_at: Instant::now(),
            };
            Ok(vec![])
        } else {
            Ok(actions)
        }
    }

    /// Applies timeout logic for the current phase.
    fn handle_timeout(&mut self) -> Result<Vec<DkgAction>> {
        match &self.state {
            DkgState::Round1Pending { started_at } => {
                if started_at.elapsed() > self.round1_timeout {
                    self.state = DkgState::Failed("Round1 timeout".to_string());
                    return Ok(vec![DkgAction::Complete(Box::new(DkgResult::Failed(
                        "Round1 timeout".to_string(),
                    )))]);
                }
            }
            DkgState::Round2Pending { started_at } => {
                if started_at.elapsed() > self.round2_timeout {
                    self.state = DkgState::Failed("Round2 timeout".to_string());
                    return Ok(vec![DkgAction::Complete(Box::new(DkgResult::Failed(
                        "Round2 timeout".to_string(),
                    )))]);
                }
            }
            DkgState::CulpritsPending { started_at } => {
                if started_at.elapsed() > self.culprits_timeout {
                    self.state = DkgState::Failed("Round2 culprits timeout".to_string());
                    return Ok(vec![DkgAction::Complete(Box::new(DkgResult::Failed(
                        "Round2 culprits timeout".to_string(),
                    )))]);
                }
            }
            _ => {}
        }

        Ok(vec![])
    }

    /// Attempts to finalize DKG and emits completion or culprits.
    fn try_finalize(&mut self) -> Result<Vec<DkgAction>> {
        let protocol = self
            .protocol
            .as_mut()
            .ok_or_else(|| anyhow::Error::new(DkgErrorKind::NoActiveProtocol))?;

        match protocol.finalize()? {
            FinalizeResult::Completed {
                key_package,
                public_key_package,
                vss_commitment,
            } => {
                self.state = DkgState::Completed;

                let config = self.config.as_ref().unwrap();
                let share = self.build_dkg_share(config, &key_package)?;

                Ok(vec![DkgAction::Complete(Box::new(DkgResult::Success(
                    Box::new(DkgCompleted {
                        public_key_package,
                        key_package: *key_package,
                        vss_commitment,
                        share,
                    }),
                )))])
            }
            FinalizeResult::Culprits(culprits) => {
                self.state = DkgState::CulpritsPending {
                    started_at: Instant::now(),
                };
                let protocol = self
                    .protocol
                    .as_ref()
                    .ok_or_else(|| anyhow::Error::new(DkgErrorKind::NoActiveProtocol))?;
                let config = self
                    .config
                    .as_ref()
                    .ok_or_else(|| anyhow::Error::new(DkgErrorKind::MissingConfig))?;
                let mut actions = Vec::new();
                for culprit in culprits.culprits.iter().copied() {
                    if let Some(offender) = protocol.address_for_identifier(culprit) {
                        actions.push(DkgAction::BroadcastComplaint(DkgComplaint {
                            session: protocol.session(),
                            complainer: config.self_address,
                            offender,
                            reason: b"round2_invalid_share".to_vec(),
                        }));
                    }
                }
                actions.insert(0, DkgAction::BroadcastRound2Culprits(culprits));
                Ok(actions)
            }
        }
    }

    /// Excludes offenders and restarts the session if quorum remains.
    fn exclude_and_restart(&mut self, offenders: Vec<Address>) -> Result<Vec<DkgAction>> {
        let mut new_excluded = vec![];
        for address in offenders {
            if self.excluded.insert(address) {
                new_excluded.push(address);
            }
        }

        if new_excluded.is_empty() {
            return Ok(vec![]);
        }

        let config = self
            .config
            .as_ref()
            .ok_or_else(|| anyhow::Error::new(DkgErrorKind::MissingConfig))?;
        let mut participants = config.validators.clone();
        participants.retain(|addr| !self.excluded.contains(addr));
        if participants.len() < config.threshold as usize {
            self.state = DkgState::Failed("Too many culprits".to_string());
            return Ok(vec![DkgAction::Complete(Box::new(DkgResult::Failed(
                "Too many culprits".to_string(),
            )))]);
        }

        let protocol_config = DkgConfig {
            session: DkgSessionId {
                era: config.era_index,
            },
            threshold: config.threshold,
            participants: participants.clone(),
            self_address: config.self_address,
        };

        let mut protocol = DkgProtocol::new(protocol_config)?;
        let round1 = protocol.generate_round1()?;
        self.protocol = Some(protocol);
        self.state = DkgState::Round1Pending {
            started_at: Instant::now(),
        };

        let config = SessionConfig {
            era_index: config.era_index,
            validators: participants,
            threshold: config.threshold,
            self_address: config.self_address,
        };
        self.config = Some(config);

        Ok(vec![DkgAction::BroadcastRound1(Box::new(round1))])
    }

    /// Builds a persisted DKG share from key package and session config.
    fn build_dkg_share(
        &self,
        config: &SessionConfig,
        key_package: &DkgKeyPackage,
    ) -> Result<DkgShare> {
        let index = config
            .validators
            .iter()
            .position(|addr| *addr == config.self_address)
            .ok_or_else(|| anyhow::Error::new(DkgErrorKind::SelfNotInValidatorsList))?;
        let index = index
            .checked_add(1)
            .and_then(|idx| u16::try_from(idx).ok())
            .ok_or_else(|| anyhow::Error::new(DkgErrorKind::ValidatorIndexOutOfRange))?;

        let signing_share = key_package.signing_share().serialize();
        let verifying_share = key_package
            .verifying_share()
            .serialize()
            .map_err(|err| anyhow!("Failed to serialize verifying share: {err}"))?;

        Ok(DkgShare {
            era: config.era_index,
            identifier: *key_package.identifier(),
            index,
            signing_share,
            verifying_share,
            threshold: *key_package.min_signers(),
        })
    }

    /// Snapshots protocol state for persistence and recovery.
    pub fn snapshot_state(&self) -> DkgSessionState {
        let Some(protocol) = self.protocol.as_ref() else {
            return DkgSessionState::default();
        };

        DkgSessionState {
            identifier_map: protocol.identifier_map(),
            round1_packages: protocol.round1_packages(),
            round2_packages: protocol.round2_packages(),
            complaints: protocol.complaints(),
            justifications: protocol.justifications(),
            round2_culprits: protocol.round2_culprit_messages(),
            completed: matches!(self.state, DkgState::Completed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DkgAction, DkgEvent, DkgState, DkgStateMachine};
    use crate::engine::dkg::{DkgConfig, DkgProtocol, FinalizeResult, SessionConfig};
    use ethexe_common::{
        Address,
        crypto::{DkgRound1, DkgRound2, DkgSessionId},
    };

    /// Builds a small deterministic validator set for tests.
    fn test_addresses() -> Vec<Address> {
        vec![
            Address::from([1; 20]),
            Address::from([2; 20]),
            Address::from([3; 20]),
        ]
    }

    #[test]
    fn state_machine_completes_rounds() {
        let participants = test_addresses();
        let self_address = participants[0];
        let session = DkgSessionId { era: 1 };
        let threshold = 2;

        let mut state_machine = DkgStateMachine::new();
        let actions = state_machine
            .process_event(DkgEvent::Start(SessionConfig {
                era_index: session.era,
                validators: participants.clone(),
                threshold,
                self_address,
            }))
            .expect("start");

        assert!(matches!(
            state_machine.state(),
            DkgState::Round1Pending { .. }
        ));

        let self_round1 = match &actions[..] {
            [DkgAction::BroadcastRound1(message)] => *message.clone(),
            other => panic!("unexpected start actions: {other:?}"),
        };

        let mut round1_messages: Vec<(Address, DkgRound1)> = vec![(self_address, self_round1)];

        let mut protocols: Vec<(Address, DkgProtocol)> = participants[1..]
            .iter()
            .map(|address| {
                (
                    *address,
                    DkgProtocol::new(DkgConfig {
                        session,
                        threshold,
                        participants: participants.clone(),
                        self_address: *address,
                    })
                    .expect("protocol init"),
                )
            })
            .collect();

        for (address, protocol) in protocols.iter_mut() {
            let round1 = protocol.generate_round1().expect("round1");
            round1_messages.push((*address, round1));
        }

        for (_, protocol) in protocols.iter_mut() {
            for (from, message) in &round1_messages {
                protocol
                    .receive_round1(*from, message.clone())
                    .expect("receive round1");
            }
        }

        let mut self_round2 = None;
        for (from, message) in round1_messages.iter().skip(1) {
            let actions = state_machine
                .process_event(DkgEvent::Round1 {
                    from: *from,
                    message: Box::new(message.clone()),
                })
                .expect("round1 event");
            for action in actions {
                if let DkgAction::BroadcastRound2(round2) = action {
                    self_round2 = Some(round2);
                }
            }
        }

        let self_round2 = self_round2.expect("self round2 action");
        assert!(matches!(
            state_machine.state(),
            DkgState::Round2Pending { .. }
        ));

        let mut round2_messages: Vec<(Address, DkgRound2)> = vec![(self_address, self_round2)];
        for (address, protocol) in protocols.iter_mut() {
            let round2 = protocol.generate_round2().expect("round2");
            round2_messages.push((*address, round2));
        }

        for (_, protocol) in protocols.iter_mut() {
            for (from, message) in &round2_messages {
                protocol
                    .receive_round2(*from, message.clone())
                    .expect("receive round2");
            }
        }

        let mut completed = false;
        for (from, message) in &round2_messages {
            let actions = state_machine
                .process_event(DkgEvent::Round2 {
                    from: *from,
                    message: Box::new(message.clone()),
                })
                .expect("round2 event");
            for action in actions {
                if let DkgAction::Complete(result) = action {
                    completed = matches!(*result, super::DkgResult::Success(_));
                }
            }
        }

        assert!(completed, "DKG should complete successfully");
        assert!(matches!(state_machine.state(), DkgState::Completed));

        for (_, protocol) in protocols.iter_mut() {
            match protocol.finalize().expect("finalize") {
                FinalizeResult::Completed { .. } => {}
                other => panic!("unexpected finalize result: {other:?}"),
            }
        }
    }
}

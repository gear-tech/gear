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

use super::{StateHandler, ValidatorState};
use crate::{
    engine::{
        EngineContext,
        prelude::{DkgEngineEvent, RoastEngineEvent},
    },
    policy::is_recoverable_roast_request_error,
    validator::adapters::handle_dkg_error,
};
use anyhow::Result;
use ethexe_common::network::VerifiedValidatorMessage;

/// Routes verified network messages into DKG/ROAST engines.
pub(crate) fn handle_verified_validator_message(
    mut state: ValidatorState,
    message: VerifiedValidatorMessage,
) -> Result<ValidatorState> {
    match message {
        VerifiedValidatorMessage::DkgRound1(msg) => {
            let era = msg.data().payload.session.era;
            let event = DkgEngineEvent::Round1 {
                from: msg.address(),
                message: Box::new(msg.data().payload.clone()),
            };
            handle_dkg_event(&mut state, era, event)?;
        }
        VerifiedValidatorMessage::DkgRound2(msg) => {
            let era = msg.data().payload.session.era;
            let event = DkgEngineEvent::Round2 {
                from: msg.address(),
                message: msg.data().payload.clone(),
            };
            handle_dkg_event(&mut state, era, event)?;
        }
        VerifiedValidatorMessage::DkgRound2Culprits(msg) => {
            let era = msg.data().payload.session.era;
            let event = DkgEngineEvent::Round2Culprits {
                from: msg.address(),
                message: msg.data().payload.clone(),
            };
            handle_dkg_event(&mut state, era, event)?;
        }
        VerifiedValidatorMessage::DkgComplaint(msg) => {
            let era = msg.data().payload.session.era;
            let event = DkgEngineEvent::Complaint {
                from: msg.address(),
                message: msg.data().payload.clone(),
            };
            handle_dkg_event(&mut state, era, event)?;
        }
        VerifiedValidatorMessage::DkgJustification(msg) => {
            let era = msg.data().payload.session.era;
            let event = DkgEngineEvent::Justification {
                from: msg.address(),
                message: msg.data().payload.clone(),
            };
            handle_dkg_event(&mut state, era, event)?;
        }
        VerifiedValidatorMessage::SignSessionRequest(msg) => {
            let request = msg.data().payload.clone();
            tracing::debug!(
                era = request.session.era,
                msg_hash = %request.msg_hash,
                leader = %request.leader,
                attempt = request.attempt,
                from = %msg.address(),
                "ROAST sign session request received"
            );
            // Delegate to ROAST engine and publish outbound messages.
            let result = state.context_mut().roast_engine.handle_event(
                RoastEngineEvent::SignSessionRequest {
                    from: msg.address(),
                    request: request.clone(),
                },
            );
            match result {
                Ok(messages) => {
                    for msg in messages {
                        state.context_mut().publish_roast_message(msg)?;
                    }
                }
                Err(err) => {
                    let era = request.session.era;
                    let recoverable = is_recoverable_roast_request_error(&err);
                    state.warning(format!("ROAST sign request failed for era {era}: {err}"));
                    if recoverable {
                        // Recoverable errors trigger a DKG restart for that era.
                        match state.context_mut().dkg_engine.restart_with(
                            era,
                            request.participants.clone(),
                            request.threshold,
                        ) {
                            Ok(actions) => {
                                state.warning(format!(
                                    "Restarting DKG for era {era} after invalid share data"
                                ));
                                for action in actions {
                                    state.context_mut().publish_dkg_action(action)?;
                                }
                            }
                            Err(restart_err) => {
                                state.warning(format!(
                                    "Failed to restart DKG for era {era}: {restart_err}"
                                ));
                            }
                        }
                    } else {
                        return Err(err);
                    }
                }
            }
        }
        VerifiedValidatorMessage::SignNonceCommit(msg) => {
            let messages =
                state
                    .context_mut()
                    .roast_engine
                    .handle_event(RoastEngineEvent::NonceCommit {
                        commit: msg.data().payload.clone(),
                    })?;
            for msg in messages {
                state.context_mut().publish_roast_message(msg)?;
            }
        }
        VerifiedValidatorMessage::SignNoncePackage(msg) => {
            let messages =
                state
                    .context_mut()
                    .roast_engine
                    .handle_event(RoastEngineEvent::NoncePackage {
                        package: msg.data().payload.clone(),
                    })?;
            for msg in messages {
                state.context_mut().publish_roast_message(msg)?;
            }
        }
        VerifiedValidatorMessage::SignShare(msg) => {
            let messages =
                state
                    .context_mut()
                    .roast_engine
                    .handle_event(RoastEngineEvent::SignShare {
                        partial: msg.data().payload.clone(),
                    })?;
            for msg in messages {
                state.context_mut().publish_roast_message(msg)?;
            }
        }
        VerifiedValidatorMessage::SignCulprits(msg) => {
            state
                .context_mut()
                .roast_engine
                .handle_event(RoastEngineEvent::SignCulprits {
                    culprits: msg.data().payload.clone(),
                })?;
        }
        VerifiedValidatorMessage::SignAggregate(msg) => {
            let aggregate = msg.data().payload.clone();
            tracing::info!(
                era = msg.data().era_index,
                msg_hash = %aggregate.msg_hash,
                "Received ROAST aggregate signature"
            );

            // Store aggregate in ROAST engine and notify coordinator if needed.
            state
                .context_mut()
                .roast_engine
                .handle_event(RoastEngineEvent::SignAggregate {
                    aggregate: aggregate.clone(),
                })?;

            if let ValidatorState::Coordinator(coordinator) = state {
                if coordinator.signing_hash == aggregate.msg_hash {
                    tracing::info!(
                        block_hash = %coordinator.batch.block_hash,
                        "✅ ROAST threshold signature completed for batch"
                    );
                    return coordinator.on_signature_complete();
                }
                return Ok(coordinator.into());
            }
        }
        _ => {
            tracing::warn!("Unexpected validator message type received");
        }
    }

    Ok(state)
}

/// Applies a DKG event and publishes any resulting outbound actions.
fn handle_dkg_event(state: &mut ValidatorState, era: u64, event: DkgEngineEvent) -> Result<()> {
    match state.context_mut().dkg_engine.handle_event(event) {
        Ok(actions) => {
            for action in actions {
                state.context_mut().publish_dkg_action(action)?;
            }
        }
        Err(err) => handle_dkg_error(state, era, err),
    }
    Ok(())
}

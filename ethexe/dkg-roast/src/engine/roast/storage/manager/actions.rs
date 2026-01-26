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

use super::types::RoastMessage;
use crate::engine::prelude::{CoordinatorAction, ParticipantAction};
use anyhow::Result;

/// Converts coordinator actions into outbound ROAST messages.
pub(super) fn coordinator_actions_to_outbound(
    actions: Vec<CoordinatorAction>,
) -> Result<Vec<RoastMessage>> {
    let mut messages = vec![];

    // Convert coordinator actions into network messages.
    for action in actions {
        match action {
            CoordinatorAction::BroadcastRequest(request) => {
                messages.push(RoastMessage::SignSessionRequest(request));
            }
            CoordinatorAction::BroadcastSigningPackage(package) => {
                messages.push(RoastMessage::SignNoncePackage(package));
            }
            CoordinatorAction::BroadcastAggregate(aggregate) => {
                messages.push(RoastMessage::SignAggregate(aggregate));
            }
            CoordinatorAction::BroadcastCulprits(culprits) => {
                messages.push(RoastMessage::SignCulprits(culprits));
            }
            CoordinatorAction::Complete(_result) => {
                // Completion is handled internally by state transitions.
                tracing::info!("ROAST signing completed");
            }
        }
    }

    Ok(messages)
}

/// Converts participant actions into outbound ROAST messages.
pub(super) fn participant_actions_to_outbound(
    actions: Vec<ParticipantAction>,
) -> Result<Vec<RoastMessage>> {
    let mut messages = vec![];

    // Convert participant actions into network messages.
    for action in actions {
        match action {
            ParticipantAction::SendNonceCommit(commit) => {
                messages.push(RoastMessage::SignNonceCommit(commit));
            }
            ParticipantAction::SendPartialSignature(partial) => {
                messages.push(RoastMessage::SignShare(partial));
            }
        }
    }

    Ok(messages)
}

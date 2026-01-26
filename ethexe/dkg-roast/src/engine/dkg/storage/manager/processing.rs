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

use super::types::ManagerState;
use crate::engine::dkg::{DkgAction, DkgEvent, DkgStateMachine};
use anyhow::Result;

/// Replays local round1/round2 broadcasts into the state machine to advance phases.
pub(super) fn apply_local_rounds(
    state: &ManagerState,
    state_machine: &mut DkgStateMachine,
    mut actions: Vec<DkgAction>,
) -> Result<Vec<DkgAction>> {
    let mut index = 0;
    while index < actions.len() {
        // Feed our own broadcasts back into the state machine.
        let follow_up = match &actions[index] {
            DkgAction::BroadcastRound1(round1) => {
                state_machine.process_event(DkgEvent::Round1 {
                    from: state.self_address,
                    message: round1.clone(),
                })?
            }
            DkgAction::BroadcastRound2(round2) => {
                state_machine.process_event(DkgEvent::Round2 {
                    from: state.self_address,
                    message: Box::new(round2.clone()),
                })?
            }
            _ => Vec::new(),
        };
        if !follow_up.is_empty() {
            actions.extend(follow_up);
        }
        index += 1;
    }
    Ok(actions)
}

/// Applies a DKG event to an existing session (if present).
pub(super) fn apply_event(
    state: &mut ManagerState,
    era: u64,
    event: DkgEvent,
) -> Result<Vec<DkgAction>> {
    if let Some(sm) = state.sessions.get_mut(&era) {
        let actions = sm.process_event(event)?;
        Ok(actions)
    } else {
        Ok(vec![])
    }
}

/// Collects timeout actions from all active sessions.
pub(super) fn collect_timeout_actions(state: &mut ManagerState) -> Result<Vec<DkgAction>> {
    let mut all_actions = Vec::new();

    for (_era, sm) in state.sessions.iter_mut() {
        let actions = sm.process_event(DkgEvent::Timeout)?;
        all_actions.extend(actions);
    }

    Ok(all_actions)
}

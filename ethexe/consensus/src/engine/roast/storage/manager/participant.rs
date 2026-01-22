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

use crate::{
    engine::prelude::{ParticipantEvent, RoastParticipant},
    policy::roast_session_id,
};
use ethexe_common::crypto::SignNoncePackage;
use std::collections::HashMap;

pub(super) fn handle_nonce_package(
    participants: &mut HashMap<crate::policy::RoastSessionId, RoastParticipant>,
    session_progress: &mut HashMap<crate::policy::RoastSessionId, super::types::SessionProgress>,
    package: SignNoncePackage,
) -> anyhow::Result<Option<Vec<super::types::RoastMessage>>> {
    let session_id = roast_session_id(package.msg_hash, package.session.era);

    if let Some(participant) = participants.get_mut(&session_id) {
        let actions = participant.process_event(ParticipantEvent::SigningPackage(package))?;
        if let Some(progress) = session_progress.get_mut(&session_id) {
            progress.last_activity = std::time::Instant::now();
        }
        return Ok(Some(super::actions::participant_actions_to_outbound(
            actions,
        )?));
    }
    Ok(None)
}

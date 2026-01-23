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

use super::super::{StateHandler, ValidatorState};
use crate::{
    engine::EngineContext,
    policy::{DkgPolicyDecision, dkg_error_policy},
};
use anyhow::Error;
use ethexe_common::db::OnChainStorageRO;

/// Applies DKG error policy and triggers restart when needed.
pub(crate) fn handle_dkg_error(s: &mut ValidatorState, era: u64, err: Error) {
    s.warning(format!("DKG processing error for era {era}: {err}"));
    if dkg_error_policy(&err) == DkgPolicyDecision::Ignore {
        return;
    }
    // Reload validators from storage to rebuild the session config.
    let Some(validators) = s.context().core.db.validators(era) else {
        s.warning(format!(
            "Unable to restart DKG for era {era}: validators missing"
        ));
        return;
    };
    let validators: Vec<_> = validators.into_iter().collect();
    let threshold = ((validators.len() as u64 * 2) / 3).max(1) as u16;
    match s
        .context_mut()
        .dkg_engine
        .restart_with(era, validators, threshold)
    {
        Ok(actions) => {
            s.warning(format!("Restarting DKG for era {era} after error"));
            for action in actions {
                if let Err(err) = s.context_mut().publish_dkg_action(action) {
                    s.warning(format!(
                        "Failed to broadcast DKG action for era {era}: {err}"
                    ));
                }
            }
        }
        Err(restart_err) => {
            s.warning(format!(
                "Failed to restart DKG for era {era}: {restart_err}"
            ));
        }
    }
}

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

use crate::{engine::storage::RoastStore, policy::dkg_session_id};
use ethexe_common::{
    Address,
    crypto::{DkgIdentifier, SignSessionRequest},
};
use std::collections::BTreeMap;

pub(super) fn ensure_sorted_participants(request: &SignSessionRequest) -> anyhow::Result<()> {
    let mut sorted_participants = request.participants.clone();
    sorted_participants.sort();
    if request.participants != sorted_participants {
        return Err(anyhow::anyhow!("Participants list must be sorted"));
    }
    Ok(())
}

pub(super) fn identifiers_for_session<DB: RoastStore>(
    db: &DB,
    era: u64,
    participants: &[Address],
) -> anyhow::Result<BTreeMap<Address, DkgIdentifier>> {
    if let Some(state) = db.dkg_session_state(dkg_session_id(era))
        && !state.identifier_map.is_empty()
    {
        let map: BTreeMap<Address, DkgIdentifier> = state.identifier_map.into_iter().collect();
        if participants.iter().all(|addr| map.contains_key(addr)) {
            return Ok(map);
        }
        return Err(anyhow::anyhow!("Missing identifiers for some participants"));
    }

    let mut sorted = participants.to_vec();
    sorted.sort();
    sorted
        .iter()
        .map(|addr| {
            let identifier = DkgIdentifier::derive(addr.as_ref())
                .map_err(|_| anyhow::anyhow!("Failed to derive identifier"))?;
            Ok((*addr, identifier))
        })
        .collect::<anyhow::Result<BTreeMap<_, _>>>()
}

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

use ethexe_common::{Address, crypto::DkgSessionId};
use gprimitives::H256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct RoastSessionId {
    pub msg_hash: H256,
    pub era: u64,
}

pub(crate) fn roast_session_id(msg_hash: H256, era: u64) -> RoastSessionId {
    RoastSessionId { msg_hash, era }
}

pub(crate) fn dkg_session_id(era: u64) -> DkgSessionId {
    DkgSessionId { era }
}

pub fn select_roast_leader(
    participants: &[Address],
    msg_hash: H256,
    era: u64,
    attempt: u32,
) -> Address {
    let mut participants = participants.to_vec();
    participants.sort();
    let mut leader = ethexe_common::crypto::frost::elect_leader(&participants, &msg_hash, era);
    for _ in 0..attempt {
        leader = ethexe_common::crypto::frost::next_leader(leader, &participants);
    }
    leader
}

pub(crate) fn is_recoverable_roast_request_error(err: &anyhow::Error) -> bool {
    use crate::engine::roast::{RoastErrorExt, RoastErrorKind};

    matches!(
        err.roast_error_kind(),
        Some(
            RoastErrorKind::MissingKeyPackage
                | RoastErrorKind::MissingDkgShare
                | RoastErrorKind::KeyPackageIdentifierMismatch
                | RoastErrorKind::KeyPackageThresholdMismatch
                | RoastErrorKind::DkgShareIndexMismatch
        )
    )
}

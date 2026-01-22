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

use super::{ParticipantConfig, ParticipantEvent, RoastParticipant};
use crate::engine::roast::core::participant::ParticipantState;
use ethexe_common::{
    Address,
    crypto::{DkgSessionId, SignNoncePackage},
};
use gprimitives::H256;

#[test]
fn signing_package_ignored_when_idle() {
    let self_address = Address::from([1; 20]);
    let mut participant = RoastParticipant::new(ParticipantConfig { self_address });
    let package = SignNoncePackage {
        session: DkgSessionId { era: 1 },
        msg_hash: H256([9; 32]),
        commitments: vec![(self_address, vec![1, 2, 3])],
    };

    let actions = participant
        .process_event(ParticipantEvent::SigningPackage(package))
        .expect("process signing package");

    assert!(actions.is_empty());
    assert!(matches!(participant.state(), ParticipantState::Idle));
}

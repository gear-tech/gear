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

use super::{DkgConfig, DkgProtocol, FinalizeResult};
use ethexe_common::{
    Address,
    crypto::{DkgRound2Culprits, DkgSessionId},
};

fn test_addresses() -> Vec<Address> {
    vec![
        Address::from([1; 20]),
        Address::from([2; 20]),
        Address::from([3; 20]),
    ]
}

#[test]
fn dkg_round1_and_round2_complete() {
    let participants = test_addresses();
    let session = DkgSessionId { era: 1 };
    let threshold = 2;

    let mut protocols: Vec<DkgProtocol> = participants
        .iter()
        .map(|address| {
            DkgProtocol::new(DkgConfig {
                session,
                threshold,
                participants: participants.clone(),
                self_address: *address,
            })
            .expect("protocol init")
        })
        .collect();

    let mut round1_messages = Vec::new();
    for protocol in &mut protocols {
        round1_messages.push(protocol.generate_round1().expect("round1"));
    }
    for protocol in &mut protocols {
        for (idx, message) in round1_messages.iter().enumerate() {
            let from = participants[idx];
            protocol
                .receive_round1(from, message.clone())
                .expect("receive round1");
        }
        assert!(protocol.is_round1_complete());
    }

    let mut round2_messages = Vec::new();
    for protocol in &mut protocols {
        round2_messages.push(protocol.generate_round2().expect("round2"));
    }
    for protocol in &mut protocols {
        for (idx, message) in round2_messages.iter().enumerate() {
            let from = participants[idx];
            protocol
                .receive_round2(from, message.clone())
                .expect("receive round2");
        }
        assert!(protocol.is_round2_complete());
    }

    for protocol in &mut protocols {
        match protocol.finalize().expect("finalize") {
            FinalizeResult::Completed { .. } => {}
            FinalizeResult::Culprits(DkgRound2Culprits { culprits, .. }) => {
                panic!("unexpected culprits: {culprits:?}");
            }
        }
    }
}

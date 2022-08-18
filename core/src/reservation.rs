// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Gas reservation structures.

use crate::ids::{MessageId, ReservationId};
use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, Encode};
use scale_info::TypeInfo;

#[derive(Debug, Clone)]
pub struct GasReserver {
    map: GasReservationMap,
    tasks: Vec<GasReservationTask>,
}

impl GasReserver {
    pub fn new(map: GasReservationMap) -> Self {
        Self {
            map,
            tasks: Vec::new(),
        }
    }

    pub fn reserve(&mut self, msg_id: MessageId, amount: u32, bn: u32) -> ReservationId {
        let idx = self.map.len();
        let id = ReservationId::generate(msg_id, idx as u64);

        let slot = GasReservationSlot { amount, bn };

        let old_amount = self.map.insert(id, slot);
        assert!(
            old_amount.is_none(),
            "reservation ID expected to be unique; qed"
        );

        self.tasks
            .push(GasReservationTask::AddReservation { id, amount, bn });

        id
    }

    pub fn unreserve(&mut self, id: ReservationId) -> Option<u32> {
        let GasReservationSlot { amount, bn } = self.map.remove(&id)?;
        self.tasks.push(GasReservationTask::RemoveNode { id, bn });
        Some(amount)
    }

    pub fn into_parts(self) -> (GasReservationMap, Vec<GasReservationTask>) {
        (self.map, self.tasks)
    }
}

#[derive(Debug, Clone)]
pub enum GasReservationTask {
    AddReservation {
        id: ReservationId,
        amount: u32,
        bn: u32,
    },
    RemoveNode {
        id: ReservationId,
        bn: u32,
    },
}

pub type GasReservationMap = BTreeMap<ReservationId, GasReservationSlot>;

#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode, TypeInfo)]
pub struct GasReservationSlot {
    amount: u32,
    bn: u32,
}

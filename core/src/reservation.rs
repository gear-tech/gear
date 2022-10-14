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
use alloc::collections::BTreeMap;
use codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Gas reserver.
#[derive(Debug, Clone)]
pub struct GasReserver {
    map: GasReservationMap,
    tasks: BTreeMap<ReservationId, GasReservationTask>,
}

impl GasReserver {
    /// Creates a new gas reserver.
    pub fn new(map: GasReservationMap) -> Self {
        Self {
            map,
            tasks: Default::default(),
        }
    }

    /// Reserves gas.
    pub fn reserve(&mut self, msg_id: MessageId, amount: u64, bn: u32) -> ReservationId {
        let idx = self.map.len();
        let id = ReservationId::generate(msg_id, idx as u64);

        let slot = GasReservationSlot { amount, bn };

        let old_amount = self.map.insert(id, slot);
        assert!(
            old_amount.is_none(),
            "reservation ID expected to be unique; qed"
        );

        let prev_task = self
            .tasks
            .insert(id, GasReservationTask::CreateReservation { amount, bn });
        assert_eq!(prev_task, None, "reservation ID collision; qed");

        id
    }

    /// Unreserves gas.
    pub fn unreserve(&mut self, id: ReservationId) -> Option<u64> {
        let GasReservationSlot { amount, bn } = self.map.remove(&id)?;
        // Only `AddReservation` task may exist here during current execution
        // so when we do unreservation we just simply remove it
        // so reservation + unreservation operations during one execution are just noop
        if self.tasks.remove(&id).is_none() {
            self.tasks
                .insert(id, GasReservationTask::RemoveReservation { bn });
        }
        Some(amount)
    }

    /// Split reserver into parts.
    pub fn into_parts(
        self,
    ) -> (
        GasReservationMap,
        BTreeMap<ReservationId, GasReservationTask>,
    ) {
        (self.map, self.tasks)
    }
}

/// Gas reservation task.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum GasReservationTask {
    /// Create a new reservation.
    CreateReservation {
        /// Amount of reserved gas.
        amount: u64,
        /// Block number until reservation will live.
        bn: u32,
    },
    /// Remove reservation.
    RemoveReservation {
        /// Block number until reservation will live.
        bn: u32,
    },
}

/// Gas reservation map.
pub type GasReservationMap = BTreeMap<ReservationId, GasReservationSlot>;

/// Gas reservation slot.
#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode, TypeInfo)]
pub struct GasReservationSlot {
    /// Amount of reserved gas.
    pub amount: u64,
    /// Block number until reservation will live.
    pub bn: u32,
}

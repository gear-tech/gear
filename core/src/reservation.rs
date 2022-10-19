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
use gear_core_errors::ExecutionError;
use scale_info::TypeInfo;

/// Gas reserver.
///
/// Controls gas reservations states.
#[derive(Debug, Clone)]
pub struct GasReserver {
    message_id: MessageId,
    nonce: u64,
    states: GasReservationStates,
}

impl GasReserver {
    /// Creates a new gas reserver.
    pub fn new(message_id: MessageId, map: GasReservationMap) -> Self {
        Self {
            message_id,
            nonce: 0,
            states: map
                .into_iter()
                .map(|(id, GasReservationSlot { amount, expiration })| {
                    (id, GasReservationState::Exists { amount, expiration })
                })
                .collect(),
        }
    }

    /// Reserves gas.
    pub fn reserve(&mut self, amount: u64, duration: u32) -> ReservationId {
        let idx = self.nonce.saturating_add(1);
        self.nonce = idx;
        let id = ReservationId::generate(self.message_id, idx);

        self.states
            .insert(id, GasReservationState::Created { amount, duration });

        id
    }

    /// Unreserves gas.
    pub fn unreserve(&mut self, id: ReservationId) -> Result<u64, ExecutionError> {
        let state = self
            .states
            .remove(&id)
            .ok_or(ExecutionError::InvalidReservationId)?;

        let amount = match state {
            GasReservationState::Exists { amount, expiration } => {
                self.states
                    .insert(id, GasReservationState::Removed { expiration });
                amount
            }
            GasReservationState::Created { amount, .. } => amount,
            GasReservationState::Removed { .. } => {
                return Err(ExecutionError::InvalidReservationId);
            }
        };

        Ok(amount)
    }

    /// Get gas reservation states.
    pub fn states(&self) -> &GasReservationStates {
        &self.states
    }

    /// Convert into gas reservation map.
    pub fn into_map<F>(self, duration_into_expiration: F) -> GasReservationMap
    where
        F: Fn(u32) -> u32,
    {
        self.states
            .into_iter()
            .flat_map(|(id, state)| match state {
                GasReservationState::Exists { amount, expiration } => {
                    Some((id, GasReservationSlot { amount, expiration }))
                }
                GasReservationState::Created { amount, duration } => {
                    let expiration = duration_into_expiration(duration);
                    Some((id, GasReservationSlot { amount, expiration }))
                }
                GasReservationState::Removed { .. } => None,
            })
            .collect()
    }
}

/// Gas reservation states.
pub type GasReservationStates = BTreeMap<ReservationId, GasReservationState>;

/// Gas reservation state.
///
/// Used to control what reservation created, removed or nothing happened.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum GasReservationState {
    /// Reservation exists.
    Exists {
        /// Amount of reserved gas.
        amount: u64,
        /// Block number when reservation will expire.
        expiration: u32,
    },
    /// Reservation will be created.
    Created {
        /// Amount of reserved gas.
        amount: u64,
        /// How many blocks reservation will live.
        duration: u32,
    },
    /// Reservation will be removed.
    Removed {
        /// Block number when reservation will expire.
        expiration: u32,
    },
}

/// Gas reservation map.
///
/// Used across execution and exists in storage.
pub type GasReservationMap = BTreeMap<ReservationId, GasReservationSlot>;

/// Gas reservation slot.
#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode, TypeInfo)]
pub struct GasReservationSlot {
    /// Amount of reserved gas.
    pub amount: u64,
    /// Block number when reservation will expire.
    pub expiration: u32,
}

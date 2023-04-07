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
use gear_core_errors::ReservationError;
use hashbrown::HashMap;
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

/// Gas reserver.
///
/// Controls gas reservations states.
#[derive(Debug, Clone)]
pub struct GasReserver {
    message_id: MessageId,
    nonce: u64,
    states: GasReservationStates,
    max_reservations: u64,
}

impl GasReserver {
    /// Creates a new gas reserver.
    pub fn new(
        message_id: MessageId,
        nonce: u64,
        map: GasReservationMap,
        max_reservations: u64,
    ) -> Self {
        Self {
            message_id,
            nonce,
            states: {
                let mut states = if map.len() <= max_reservations as usize {
                    HashMap::with_capacity(max_reservations as usize)
                } else {
                    HashMap::new()
                };
                states.extend(map.into_iter().map(|(id, slot)| (id, slot.into())));
                states
            },
            max_reservations,
        }
    }

    fn check_execution_limit(&self) -> Result<(), ReservationError> {
        // operation might very expensive in the future
        // so we will store 2 numerics to optimize it maybe
        let current_reservations = self
            .states
            .values()
            .map(|state| {
                matches!(
                    state,
                    GasReservationState::Exists { .. } | GasReservationState::Created { .. }
                ) as u64
            })
            .sum::<u64>();
        if current_reservations > self.max_reservations {
            Err(ReservationError::ReservationsLimitReached)
        } else {
            Ok(())
        }
    }

    fn fetch_inc_nonce(&mut self) -> u64 {
        let nonce = self.nonce;
        self.nonce = nonce.saturating_add(1);
        nonce
    }

    /// Reserves gas.
    pub fn reserve(
        &mut self,
        amount: u64,
        duration: u32,
    ) -> Result<ReservationId, ReservationError> {
        self.check_execution_limit()?;

        let id = ReservationId::generate(self.message_id, self.fetch_inc_nonce());

        self.states.insert(
            id,
            GasReservationState::Created {
                amount,
                duration,
                used: false,
            },
        );

        Ok(id)
    }

    /// Unreserves gas.
    pub fn unreserve(&mut self, id: ReservationId) -> Result<u64, ReservationError> {
        let state = self
            .states
            .remove(&id)
            .ok_or(ReservationError::InvalidReservationId)?;

        let amount = match state {
            GasReservationState::Exists { amount, finish, .. } => {
                self.states
                    .insert(id, GasReservationState::Removed { expiration: finish });
                amount
            }
            GasReservationState::Created { amount, .. } => amount,
            GasReservationState::Removed { .. } => {
                return Err(ReservationError::InvalidReservationId);
            }
        };

        Ok(amount)
    }

    /// Marks reservation as used to avoid double usage in sys-calls like `gr_reservation_send`.
    pub fn mark_used(&mut self, id: ReservationId) -> Result<(), ReservationError> {
        if let Some(
            GasReservationState::Created { used, .. } | GasReservationState::Exists { used, .. },
        ) = self.states.get_mut(&id)
        {
            if *used {
                Err(ReservationError::InvalidReservationId)
            } else {
                *used = true;
                Ok(())
            }
        } else {
            Err(ReservationError::InvalidReservationId)
        }
    }

    /// Return reservation nonce.
    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    /// Get gas reservation states.
    pub fn states(&self) -> &GasReservationStates {
        &self.states
    }

    /// Convert into gas reservation map.
    pub fn into_map<F>(
        self,
        current_block_height: u32,
        duration_into_expiration: F,
    ) -> GasReservationMap
    where
        F: Fn(u32) -> u32,
    {
        self.states
            .into_iter()
            .flat_map(|(id, state)| match state {
                GasReservationState::Exists {
                    amount,
                    start,
                    finish,
                    ..
                } => Some((
                    id,
                    GasReservationSlot {
                        amount,
                        start,
                        finish,
                    },
                )),
                GasReservationState::Created {
                    amount, duration, ..
                } => {
                    let expiration = duration_into_expiration(duration);
                    Some((
                        id,
                        GasReservationSlot {
                            amount,
                            start: current_block_height,
                            finish: expiration,
                        },
                    ))
                }
                GasReservationState::Removed { .. } => None,
            })
            .collect()
    }
}

/// Gas reservation states.
pub type GasReservationStates = HashMap<ReservationId, GasReservationState>;

/// Gas reservation state.
///
/// Used to control what reservation created, removed or nothing happened.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum GasReservationState {
    /// Reservation exists.
    Exists {
        /// Amount of reserved gas.
        amount: u64,
        /// Block number when reservation is crated.
        start: u32,
        /// Block number when reservation will expire.
        finish: u32,
        /// Whether reservation used.
        used: bool,
    },
    /// Reservation will be created.
    Created {
        /// Amount of reserved gas.
        amount: u64,
        /// How many blocks reservation will live.
        duration: u32,
        /// Whether reservation used.
        used: bool,
    },
    /// Reservation will be removed.
    Removed {
        /// Block number when reservation will expire.
        expiration: u32,
    },
}

impl From<GasReservationSlot> for GasReservationState {
    fn from(slot: GasReservationSlot) -> Self {
        Self::Exists {
            amount: slot.amount,
            start: slot.start,
            finish: slot.finish,
            used: false,
        }
    }
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
    /// Block number when reservation is crated.
    pub start: u32,
    /// Block number when reservation will expire.
    pub finish: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_reserver() -> GasReserver {
        GasReserver::new(Default::default(), 0, Default::default(), 256)
    }

    #[test]
    #[should_panic = "LimitReached"]
    fn max_reservations_limit_works() {
        let mut reserver = new_reserver();
        for _ in 0..usize::MAX {
            reserver.reserve(100, 10).unwrap();
        }
    }

    #[test]
    fn mark_used_twice_fails() {
        let mut reserver = new_reserver();
        let id = reserver.reserve(1, 1).unwrap();
        reserver.mark_used(id).unwrap();
        assert_eq!(
            reserver.mark_used(id),
            Err(ReservationError::InvalidReservationId)
        );

        // not found
        assert_eq!(
            reserver.mark_used(ReservationId::default()),
            Err(ReservationError::InvalidReservationId)
        );
    }

    #[test]
    fn remove_reservation_twice_fails() {
        let mut reserver = new_reserver();
        let id = reserver.reserve(1, 1).unwrap();
        reserver.unreserve(id).unwrap();
        assert_eq!(
            reserver.unreserve(id),
            Err(ReservationError::InvalidReservationId)
        );
    }
}

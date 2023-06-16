// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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
    /// Message id within which reservations are created
    /// by the current instance of [`GasReserver`].
    message_id: MessageId,
    /// Nonce used to generate [`ReservationId`]s.
    ///
    /// It's really important that if gas reserver is created
    /// several times with the same `message_id`, value of this
    /// field is re-used.
    // TODO #2773
    nonce: u64,
    /// Gas reservations states.
    states: GasReservationStates,
    /// Maximum allowed reservations to be stored in `states`.
    ///
    /// This field is used not only to control `states` during
    /// one execution, but also during several execution using
    /// gas reserver for the actor. To reach that `states` must
    /// be set with reservation from previous executions of the
    /// actor.
    max_reservations: u64,
}

impl GasReserver {
    /// Creates a new gas reserver.
    ///
    /// `map`, which is a [`BTreeMap`] of [`GasReservationSlot`]s,
    /// will be converted to the [`HashMap`] of [`GasReservationState`]s.
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
                let mut states = HashMap::with_capacity(max_reservations as usize);
                states.extend(map.into_iter().map(|(id, slot)| (id, slot.into())));
                states
            },
            max_reservations,
        }
    }

    /// Checks that the number of existing and newly created reservations
    /// in the `states` is less than `max_reservations`. Removed reservations,
    /// which are stored with the [`GasReservationState::Removed`] state in the
    /// `states`, aren't excluded from the check.
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

    /// Fetches current state of the `nonce` and
    /// updates its state by incrementing it.
    fn fetch_inc_nonce(&mut self) -> u64 {
        let nonce = self.nonce;
        self.nonce = nonce.saturating_add(1);
        nonce
    }

    /// Reserves gas.
    ///
    /// Creates a new reservation and returns its id.
    ///
    /// Returns an error if maximum limit of reservations is reached.
    pub fn reserve(
        &mut self,
        amount: u64,
        duration: u32,
    ) -> Result<ReservationId, ReservationError> {
        self.check_execution_limit()?;

        let id = ReservationId::generate(self.message_id, self.fetch_inc_nonce());

        // TODO #2773
        let maybe_reservation = self.states.insert(
            id,
            GasReservationState::Created {
                amount,
                duration,
                used: false,
            },
        );

        if maybe_reservation.is_some() {
            unreachable!(
                "Duplicate reservation was created with message id {} and nonce {}",
                self.message_id, self.nonce,
            );
        }

        Ok(id)
    }

    /// Unreserves gas reserved withing `id` reservation.
    ///
    /// Return error if:
    /// 1. Reservation doesn't exist.
    /// 2. Reservation was "unreserved", so in [`GasReservationState::Removed`] state.
    /// 3. Reservation was marked used.
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

    /// Marks reservation as used.
    ///
    /// This allows to avoid double usage of the reservation
    /// for sending a new message from execution of `message_id`
    /// of current gas reserver.
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

    /// Returns gas reservations current nonce.
    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    /// Gets gas reservations states.
    pub fn states(&self) -> &GasReservationStates {
        &self.states
    }

    /// Converts current gas reserver into gas reservation map.
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

/// Gas reservations states.
pub type GasReservationStates = HashMap<ReservationId, GasReservationState>;

/// Gas reservation state.
///
/// Used to control whether reservation was created, removed or nothing happened.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum GasReservationState {
    /// Reservation exists.
    Exists {
        /// Amount of reserved gas.
        amount: u64,
        /// Block number when reservation is created.
        start: u32,
        /// Block number when reservation will expire.
        finish: u32,
        /// Flag signalizing whether reservation is used.
        used: bool,
    },
    /// Reservation will be created.
    Created {
        /// Amount of reserved gas.
        amount: u64,
        /// How many blocks reservation will live.
        duration: u32,
        /// Flag signalizing whether reservation is used.
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

/// Gas reservations map.
///
/// Used across execution and is stored to storage.
pub type GasReservationMap = BTreeMap<ReservationId, GasReservationSlot>;

/// Gas reservation slot.
#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode, TypeInfo)]
pub struct GasReservationSlot {
    /// Amount of reserved gas.
    pub amount: u64,
    /// Block number when reservation is created.
    pub start: u32,
    /// Block number when reservation will expire.
    pub finish: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_RESERVATIONS: u64 = 256;

    fn new_reserver() -> GasReserver {
        GasReserver::new(Default::default(), 0, Default::default(), MAX_RESERVATIONS)
    }

    #[test]
    fn max_reservations_limit_works() {
        let mut reserver = new_reserver();
        for n in 0..(MAX_RESERVATIONS * 10) {
            let res = reserver.reserve(100, 10);
            if n > MAX_RESERVATIONS {
                assert_eq!(res, Err(ReservationError::ReservationsLimitReached));
            } else {
                assert!(res.is_ok());
            }
        }
    }

    #[test]
    fn mark_used_for_unreserved_fails() {
        let mut reserver = new_reserver();
        let id = reserver.reserve(1, 1).unwrap();
        reserver.unreserve(id).unwrap();

        assert_eq!(
            reserver.mark_used(id),
            Err(ReservationError::InvalidReservationId)
        );
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

    #[test]
    fn remove_non_existing_reservation_fails() {
        let id = ReservationId::from([0xff; 32]);

        let mut map = GasReservationMap::new();
        map.insert(
            id,
            GasReservationSlot {
                amount: 1,
                start: 1,
                finish: 100,
            },
        );

        let mut reserver = GasReserver::new(Default::default(), 0, map, 256);
        reserver.unreserve(id).unwrap();

        assert_eq!(
            reserver.unreserve(id),
            Err(ReservationError::InvalidReservationId)
        );
    }
}

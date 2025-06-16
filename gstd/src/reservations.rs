// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

//! Gear reservation manager implementation.
//!
//! It is capable of managing multiple gas reservations
//! providing simple interface to user.

use crate::prelude::*;
use gcore::errors::Result;
use parity_scale_codec::{Decode, Encode};

#[cfg(not(test))]
use crate::exec;
#[cfg(test)]
use tests::exec_mock as exec;

#[cfg(not(test))]
use crate::ReservationId;
#[cfg(test)]
use tests::ReservationIdMock as ReservationId;

/// Stores additional data along with [`ReservationId`] to track its state.
#[derive(Clone, Copy, Debug, TypeInfo, Hash, Encode, Decode)]
pub struct Reservation {
    id: ReservationId,
    amount: u64,
    valid_until: u32,
}

impl From<Reservation> for ReservationId {
    fn from(res: Reservation) -> Self {
        res.id
    }
}

impl Reservation {
    /// Reserve the `amount` of gas for further usage.
    ///
    /// `duration` is the block count within which the reserve must be used.
    ///
    /// Refer to [`ReservationId`] for the more detailed description.
    pub fn reserve(amount: u64, duration: u32) -> Result<Self> {
        let block_height = exec::block_height();

        Ok(Self {
            id: ReservationId::reserve(amount, duration)?,
            amount,
            valid_until: duration.saturating_add(block_height),
        })
    }

    /// Unreserve unused gas from the reservation.
    ///
    /// If successful, it returns the reserved amount of gas.
    pub fn unreserve(self) -> Result<u64> {
        self.id.unreserve()
    }

    /// `ReservationId` associated with current `Reservation`.
    pub fn id(&self) -> ReservationId {
        self.id
    }

    /// Amount of gas stored inside this `Reservation`.
    pub fn amount(&self) -> u64 {
        self.amount
    }

    /// Returns block number when this `Reservation` expires.
    pub fn valid_until(&self) -> u32 {
        self.valid_until
    }
}

/// Reservation manager.
///
/// The manager is used to control multiple gas reservations
/// across executions. It can be used when you only care about
/// reserved amounts and not concrete [`ReservationId`]s.
///
/// # Examples
///
/// Create gas reservations inside `init` and use them inside `handle`.
///
/// ```
/// use gstd::{msg, prelude::*, Reservations};
///
/// static mut RESERVATIONS: Reservations = Reservations::new();
///
/// #[unsafe(no_mangle)]
/// extern "C" fn init() {
///     unsafe {
///         RESERVATIONS
///             .reserve(200_000, 50)
///             .expect("failed to reserve gas");
///         RESERVATIONS
///             .reserve(100_000, 100)
///             .expect("failed to reserve gas");
///         RESERVATIONS
///             .reserve(50_000, 30)
///             .expect("failed to reserve gas");
///     }
/// }
///
/// #[unsafe(no_mangle)]
/// extern "C" fn handle() {
///     let reservation = unsafe { RESERVATIONS.try_take_reservation(100_000) };
///     if let Some(reservation) = reservation {
///         msg::send_bytes_from_reservation(
///             reservation.id(),
///             msg::source(),
///             "send_bytes_from_reservation",
///             0,
///         )
///         .expect("Failed to send message from reservation");
///     } else {
///         msg::send_bytes(msg::source(), "send_bytes", 0).expect("Failed to send message");
///     }
/// }
/// ```
///
/// # See also
/// - [`ReservationId`](ReservationId) is used to reserve and unreserve gas for
///   program execution later.
/// - [`Reservation`] stores some additional data along with `ReservationId`.
#[derive(Default, Clone, Debug, TypeInfo, Hash, Encode, Decode)]
pub struct Reservations(Vec<Reservation>);

impl Reservations {
    /// Create a new [`Reservations`] struct.
    pub const fn new() -> Self {
        Reservations(Vec::new())
    }

    /// Reserve the `amount` of gas for further usage.
    ///
    /// `duration` is the block count within which the reservation must be used.
    ///
    /// # Underlying logics
    ///
    /// Executes for O(logN)..O(N), where N is a number of stored
    /// reservations.
    ///
    /// All the reservations are kept sorted by amount in
    /// ascending order(when amount is the same, they're sorted by time when
    /// they expire) when inserted, so the closer inserted element to the
    /// beginning of the underlying `Vec` the closer execution time will be
    /// to O(N).
    ///
    /// Also, when the underlying `Vec` will allocate new memory
    /// the attempt to clean expired reservations occurs to avoid memory
    /// allocations.
    pub fn reserve(&mut self, amount: u64, duration: u32) -> Result<()> {
        let new_reservation = Reservation::reserve(amount, duration)?;

        let insert_range_start = self
            .0
            .partition_point(|reservation| reservation.amount < amount);
        let insert_range_end = self
            .0
            .partition_point(|reservation| reservation.amount <= amount);

        let insert_to =
            self.0[insert_range_start..insert_range_end].binary_search_by(|reservation| {
                reservation.valid_until.cmp(&new_reservation.valid_until)
            });
        let insert_to = if insert_range_start == self.0.len() {
            self.0.len()
        } else {
            match insert_to {
                Ok(pos) => pos + 1,
                Err(pos) => pos,
            }
        };

        // self.0 will allocate new memory.
        if self.0.capacity() == self.0.len() {
            self.cleanup();
        }

        self.0.insert(insert_to, new_reservation);

        Ok(())
    }

    /// Find the appropriate reservation with reserved amount greater than or
    /// equal to `amount`.
    ///
    /// If such a reservation is found, [`Reservation`] is returned.
    ///
    /// # Underlying logics
    ///
    /// Executes for O(logN)..O(N), where N is amount of stored
    /// reservations. When there's many expired reservations execution time
    /// is closer to O(N).
    ///
    /// All the reservations are sorted by their amount and then by the time
    /// when they'll expire (both are in ascending order) in underlying `Vec`,
    /// so when one's trying to take reservation, reservation with the least
    /// possible amount is found and if it's already expired, the search to the
    /// end of underlying `Vec` occurs. After that, all the expired
    /// reservations that were found in process of search are cleaned out.
    ///
    /// # See also
    /// - [`ReservationId`] is used to reserve and unreserve gas amount for
    ///   program execution later.
    /// - [`Reservation`] stores some additional data along with
    ///   `ReservationId`.
    pub fn try_take_reservation(&mut self, amount: u64) -> Option<Reservation> {
        let search_from = self
            .0
            .partition_point(|reservation| reservation.amount < amount);

        if search_from < self.0.len() {
            let block_height = exec::block_height();
            for i in search_from..self.0.len() {
                if self.0[i].valid_until > block_height {
                    // All the checked reservations are already expired at this time.
                    let suitable = self
                        .0
                        .drain(search_from..=i)
                        .next_back()
                        .expect("At least one element in range");

                    return Some(suitable);
                }
            }
        }

        // All the checked reservations are already expired at this time.
        self.0.drain(search_from..);

        None
    }

    /// Returns an amount of the stored reservations
    /// that aren't expired at this time.
    ///
    /// Executes for O(N) where N is amount of stored reservations.
    pub fn count_valid(&self) -> usize {
        let block_height = exec::block_height();
        self.0
            .iter()
            .filter(|reservation| reservation.valid_until > block_height)
            .count()
    }

    /// Returns an amount of all the stored reservations (including expired).
    pub fn count_all(&self) -> usize {
        self.0.len()
    }

    fn cleanup(&mut self) {
        let block_height = exec::block_height();

        self.0 = self
            .0
            .drain(..)
            .filter(|reservation| reservation.valid_until > block_height)
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use crate::{prelude::*, Reservations};
    use gcore::errors::Result;
    use parity_scale_codec::{Decode, Encode};

    #[must_use]
    #[derive(Clone, Copy, Debug, TypeInfo, Hash, Encode, Decode)]
    pub struct ReservationIdMock {
        pub valid_until: u32,
        pub amount: u64,
    }

    impl ReservationIdMock {
        pub fn reserve(amount: u64, duration: u32) -> Result<ReservationIdMock> {
            Ok(ReservationIdMock {
                valid_until: duration + exec_mock::block_height(),
                amount,
            })
        }

        pub fn unreserve(self) -> Result<u64> {
            unreachable!()
        }

        fn is_valid(&self) -> bool {
            self.valid_until > exec_mock::block_height()
        }
    }

    pub mod exec_mock {
        static mut BLOCK_HEIGHT: u32 = 0;

        pub fn block_height() -> u32 {
            unsafe { BLOCK_HEIGHT }
        }

        pub(super) fn set_block_height(block_height: u32) {
            unsafe {
                BLOCK_HEIGHT = block_height;
            }
        }
    }

    #[test]
    fn reservations_expire() -> Result<()> {
        exec_mock::set_block_height(0);

        let mut reservations = Reservations::new();
        reservations.reserve(10_000, 5)?;
        reservations.reserve(10_000, 10)?;

        exec_mock::set_block_height(5);

        assert_eq!(reservations.count_all(), 2);
        assert_eq!(reservations.count_valid(), 1);

        let reservation = reservations.try_take_reservation(10_000);
        assert_eq!(reservation.map(|res| res.id().is_valid()), Some(true));

        Ok(())
    }

    #[test]
    fn the_best_possible_reservation_taken() -> Result<()> {
        exec_mock::set_block_height(0);

        let mut reservations = Reservations::new();

        reservations.reserve(10_000, 5)?;
        reservations.reserve(10_000, 10)?;
        reservations.reserve(10_000, 15)?;
        exec_mock::set_block_height(7);

        let reservation = reservations.try_take_reservation(10_000);
        // The shortest possible living reservation taken.
        assert_eq!(reservation.map(|res| res.id().valid_until), Some(10));

        let reservation = reservations.try_take_reservation(10_000);
        assert_eq!(reservation.map(|res| res.id().valid_until), Some(15));

        assert_eq!(reservations.count_valid(), 0);

        reservations.reserve(10_000, 100)?;
        reservations.reserve(20_000, 100)?;
        reservations.reserve(30_000, 100)?;

        let reservation = reservations.try_take_reservation(1_000);
        // Reservation with the smallest amount is taken.
        assert_eq!(reservation.map(|res| res.id.amount), Some(10_000));

        let reservation = reservations.try_take_reservation(1_000);
        // Reservation with the smallest amount is taken.
        assert_eq!(reservation.map(|res| res.id.amount), Some(20_000));

        Ok(())
    }
}

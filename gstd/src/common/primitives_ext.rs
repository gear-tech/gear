// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::ReservationId;
use gcore::errors::Result;

mod private {
    use crate::ReservationId;

    pub trait Sealed {}

    impl Sealed for ReservationId {}
}

/// Reservation identifier extension.
///
/// The identifier is used to reserve and unreserve gas amount for program
/// execution later.
///
/// # Examples
///
/// ```rust,ignore
/// use gstd::{prelude::*, ReservationId};
///
/// static mut RESERVED: Option<ReservationId> = None;
///
/// #[unsafe(no_mangle)]
/// extern "C" fn init() {
///     let reservation_id = ReservationId::reserve(50_000_000, 7).expect("Unable to reserve");
///     unsafe { RESERVED = Some(reservation_id) };
/// }
///
/// #[unsafe(no_mangle)]
/// extern "C" fn handle() {
///     let reservation_id = unsafe { RESERVED.take().expect("Empty `RESERVED`") };
///     reservation_id.unreserve().expect("Unable to unreserve");
/// }
/// ```
pub trait ReservationIdExt: private::Sealed + Sized {
    /// Reserve the `amount` of gas for further usage.
    ///
    /// `duration` is the block count within which the reserve must be used.
    ///
    /// This function returns [`ReservationId`], which one can use for gas
    /// unreserving.
    ///
    /// # Examples
    ///
    /// Reserve 50 million of gas for one block, send a reply, then unreserve
    /// gas back:
    ///
    /// ```
    /// use gstd::{ReservationId, msg, prelude::*};
    ///
    /// #[unsafe(no_mangle)]
    /// extern "C" fn handle() {
    ///     let reservation_id = ReservationId::reserve(50_000_000, 1).expect("Unable to reserve");
    ///     msg::reply_bytes_from_reservation(reservation_id.clone(), b"PONG", 0)
    ///         .expect("Unable to reply");
    ///     let reservation_left = reservation_id.unreserve().expect("Unable to unreserve");
    /// }
    /// ```
    fn reserve(amount: u64, duration: u32) -> Result<Self>;

    /// Unreserve unused gas from the reservation.
    ///
    /// If successful, it returns the reserved amount of gas.
    fn unreserve(self) -> Result<u64>;
}

impl ReservationIdExt for ReservationId {
    fn reserve(amount: u64, duration: u32) -> Result<Self> {
        gcore::exec::reserve_gas(amount, duration)
    }

    fn unreserve(self) -> Result<u64> {
        gcore::exec::unreserve_gas(self)
    }
}

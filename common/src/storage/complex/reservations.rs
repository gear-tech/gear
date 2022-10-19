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

use crate::storage::MapStorage;
use std::marker::PhantomData;

pub trait ReservationPool {
    /// Reservation ID.
    type ReservationId;

    /// Block number type.
    type BlockNumber;

    /// Inner error type of queue storing algorithm.
    type Error: ReservationPoolError;

    /// Output error type of the queue.
    type OutputError: From<Self::Error>;

    /// Inserts given reservation in pool.
    fn add(id: Self::ReservationId, bn: Self::BlockNumber) -> Result<(), Self::OutputError>;

    /// Gets given block number associated with reservation ID.
    fn get(id: Self::ReservationId) -> Result<Self::BlockNumber, Self::OutputError>;

    /// Removes all tasks from reservation pool.
    fn clear();

    /// Returns bool, defining does reservation exist in pool.
    fn contains(id: &Self::ReservationId) -> bool;

    /// Removes reservation from pool by given key,
    /// if present, else returns error.
    fn delete(id: Self::ReservationId) -> Result<Self::BlockNumber, Self::OutputError>;
}

/// Represents reservation pool error type.
///
/// Contains constructors for all existing errors.
pub trait ReservationPoolError {
    /// Occurs when given reservation already exists in pool.
    fn duplicate_reservation() -> Self;

    /// Occurs when reservation wasn't found in storage.
    fn reservation_not_found() -> Self;
}

pub struct ReservationPoolImpl<T, BlockNumber, ReservationId, Error, OutputError>(
    PhantomData<(T, BlockNumber, ReservationId, Error, OutputError)>,
);

impl<T, BlockNumber, ReservationId, Error, OutputError> ReservationPool
    for ReservationPoolImpl<T, BlockNumber, ReservationId, Error, OutputError>
where
    T: MapStorage<Key = ReservationId, Value = BlockNumber>,
    Error: ReservationPoolError,
    OutputError: From<Error>,
{
    type ReservationId = ReservationId;
    type BlockNumber = BlockNumber;
    type Error = Error;
    type OutputError = OutputError;

    fn add(id: Self::ReservationId, bn: Self::BlockNumber) -> Result<(), Self::OutputError> {
        if !Self::contains(&id) {
            T::insert(id, bn);
            Ok(())
        } else {
            Err(Self::Error::duplicate_reservation().into())
        }
    }

    fn get(id: Self::ReservationId) -> Result<Self::BlockNumber, Self::OutputError> {
        T::get(&id).ok_or_else(|| Self::Error::reservation_not_found().into())
    }

    fn clear() {
        T::clear();
    }

    fn contains(id: &Self::ReservationId) -> bool {
        T::contains_key(id)
    }

    fn delete(id: Self::ReservationId) -> Result<Self::BlockNumber, Self::OutputError> {
        if let Some(bn) = T::get(&id) {
            T::remove(id);
            Ok(bn)
        } else {
            Err(Self::Error::reservation_not_found().into())
        }
    }
}

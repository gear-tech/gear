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

//! This module provides [`TxOutput`] helper type.

use subxt::{blocks::ExtrinsicEvents, utils::H256};

use crate::{AsGear, Error, Event, GearConfig, Result, TxInBlock};

/// Transaction with its output value.
#[derive(Debug, derive_more::AsRef, derive_more::AsMut)]
pub struct TxOutput<T = ()> {
    /// The hash of the block that the transaction has made it into.
    pub block_hash: H256,

    /// The hash of the extrinsic that was submitted.
    pub extrinsic_hash: H256,

    /// Events associated with the transaction.
    pub events: ExtrinsicEvents<GearConfig>,

    /// Output value of the transaction.
    #[as_ref]
    #[as_mut]
    pub value: T,
}

/// [`TxOutput`] without value.
impl TxOutput {
    /// Constructs a [`TxOutput`] without value from a [`TxInBlock`].
    pub async fn new(tx: TxInBlock) -> Result<Self> {
        Ok(Self {
            block_hash: tx.block_hash(),
            extrinsic_hash: tx.extrinsic_hash(),
            events: tx.wait_for_success().await?,
            value: (),
        })
    }

    /// Finds an event with an output value and extracts the value
    /// from the event.
    ///
    /// Essentially just an [`Iterator::filter_map`] on events.
    pub fn find_map<T, F>(self, mut f: F) -> Result<TxOutput<Option<T>>>
    where
        F: FnMut(Event) -> Option<T>,
    {
        let value = self
            .events
            .iter()
            .map(move |event| event?.as_gear().map(&mut f))
            .find_map(|res| res.transpose())
            .transpose()?;

        Ok(self.with_value(value))
    }

    /// Filters and maps events and collects results into [`Vec`] as a value.
    ///
    /// Essentially just a [`Iterator::filter_map`] on events.
    pub fn filter_map<T, F>(self, mut f: F) -> Result<TxOutput<Vec<T>>>
    where
        F: FnMut(Event) -> Option<T>,
    {
        let values = self
            .events
            .iter()
            .map(move |event| event?.as_gear().map(&mut f))
            .filter_map(|res| res.transpose())
            .collect::<Result<Vec<_>>>()?;

        Ok(self.with_value(values))
    }

    /// Ensures that there's an event matching given predicate.
    ///
    /// Essentially just an [`Interator::any`] on events.
    pub fn any<F>(self, mut f: F) -> Result<TxOutput<Option<()>>>
    where
        F: FnMut(Event) -> bool,
    {
        self.find_map(move |event| f(event).then_some(()))
    }

    /// Validates events associated with the transaction.
    pub fn validate_events<E, F>(self, mut f: F) -> Result<Self, E>
    where
        E: From<Error>,
        F: FnMut(Event) -> Result<(), E>,
    {
        self.events
            .iter()
            .try_for_each(move |event| f(event.map_err(Error::from)?.as_gear()?))?;

        Ok(self)
    }
}

/// [`TxOutput`] withut value, but after some kind of validation.
///
/// Logically equivalent to [`TxOutput<bool>`].
impl TxOutput<Option<()>> {
    /// Applies logical `||` on the value and `b`.
    ///
    /// Returns:
    /// - `Some(())` if `self` is `Some(())` or `b` is `true`
    /// - `None` otherwise.
    pub fn or(self, b: bool) -> Self {
        self.map(|opt| opt.or(b.then_some(())))
    }

    /// Maps unit inside the inner [`Option`].
    pub fn then<T, F>(self, f: F) -> TxOutput<Option<T>>
    where
        F: FnOnce() -> T,
    {
        self.map(move |opt| opt.map(|()| f()))
    }
}

impl<T> TxOutput<T> {
    /// Replaces the inner value.
    pub fn with_value<O>(self, value: O) -> TxOutput<O> {
        TxOutput {
            block_hash: self.block_hash,
            extrinsic_hash: self.extrinsic_hash,
            events: self.events,
            value,
        }
    }

    /// Separates the value and the transaction details.
    ///
    /// Useful for taking the value ownership out of the [`TxOutput`].
    pub fn split(self) -> (TxOutput, T) {
        (
            TxOutput {
                block_hash: self.block_hash,
                extrinsic_hash: self.extrinsic_hash,
                events: self.events,
                value: (),
            },
            self.value,
        )
    }

    /// Maps inner value of the [`TxOutput`].
    pub fn map<O, F>(self, f: F) -> TxOutput<O>
    where
        F: FnOnce(T) -> O,
    {
        let (tx_output, value) = self.split();
        tx_output.with_value(f(value))
    }
}

impl<T> TxOutput<Option<T>> {
    /// Applies [`Option::or_else`] to the inner [`Option`].
    pub fn or_else<F>(self, f: F) -> Self
    where
        F: FnOnce() -> Option<T>,
    {
        self.map(move |opt| opt.or_else(f))
    }

    /// Unwraps the inner `Option`.
    ///
    /// Returns `Err(Error::EventNotFound)` if it's `None`.
    pub fn ok_or_err(self) -> Result<TxOutput<T>> {
        let (tx_output, option) = self.split();
        match option {
            Some(value) => Ok(tx_output.with_value(value)),
            None => Err(Error::EventNotFound),
        }
    }
}

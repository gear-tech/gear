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

//! Utilities used in message implementations.

use gprimitives::{ActorId, MessageId};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// Incrementable nonce.
///
/// This is useful for generating unique IDs for messages or other entities.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Encode,
    Decode,
    MaxEncodedLen,
    TypeInfo,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct IncrementNonce(u32);

impl IncrementNonce {
    /// Creates a new increment nonce with an initial value of 0.
    pub const fn new() -> Self {
        Self(0)
    }

    /// Creates a new increment nonce with the given initial value.
    ///
    /// For testing purposes only.
    #[cfg(feature = "test-utils")]
    pub const fn from(value: u32) -> Self {
        Self(value)
    }

    /// Returns the current nonce and increments it by 1 for future use.
    /// Uses saturating addition to prevent overflow.
    pub fn fetch_inc(&mut self) -> u32 {
        let current = self.0;
        self.0 = current.saturating_add(1);
        current
    }
}

/// Wrapper for a message along with its destination actor ID.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Encode,
    Decode,
    MaxEncodedLen,
    TypeInfo,
    derive_more::Deref,
    derive_more::DerefMut,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct WithDestination<T> {
    #[deref]
    #[deref_mut]
    inner: T,

    destination: ActorId,
}

impl<T> WithDestination<T> {
    /// Creates a new instance of `WithDestination` with the given inner value
    /// and destination actor ID.
    pub const fn new(inner: T, destination: ActorId) -> Self {
        Self { inner, destination }
    }

    /// Returns the destination actor ID of the message.
    pub fn destination(&self) -> ActorId {
        self.destination
    }

    /// Converts the inner type to another type using the `From` trait.
    pub fn convert<U: From<T>>(self) -> WithDestination<U> {
        WithDestination::new(self.inner.into(), self.destination)
    }

    /// Decomposes `self` into the inner data.
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Decomposes `self` into the inner data and the destination actor ID.
    pub fn into_parts(self) -> (T, ActorId) {
        (self.inner, self.destination)
    }
}

/// Helper trait for `WithDestination` struct interoperability.
pub trait WrapWithDestination: Sized {
    /// Creates a new instance of `WithDestination` with the given
    /// inner and message destination.
    fn with_destination(self, destination: ActorId) -> WithDestination<Self> {
        WithDestination::new(self, destination)
    }

    /// Wraps the given inner value with a destination actor ID
    /// and converts it to the specified type.
    fn with_destination_into<T: From<WithDestination<Self>>>(self, destination: ActorId) -> T {
        T::from(self.with_destination(destination))
    }
}

/// Wrapper for a message along with its ID.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Encode,
    Decode,
    MaxEncodedLen,
    TypeInfo,
    derive_more::Deref,
    derive_more::DerefMut,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct WithId<T> {
    #[deref]
    #[deref_mut]
    inner: T,

    id: MessageId,
}

impl<T> WithId<T> {
    /// Creates a new instance of `WithId` with the given inner value
    /// and message ID.
    pub const fn new(inner: T, id: MessageId) -> Self {
        Self { inner, id }
    }

    /// Returns the ID of the message.
    pub fn id(&self) -> MessageId {
        self.id
    }

    /// Converts the inner type to another type using the `From` trait.
    pub fn convert<U: From<T>>(self) -> WithId<U> {
        WithId::new(self.inner.into(), self.id)
    }

    /// Decomposes `self` into the inner data.
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Decomposes `self` into the inner data and the message ID.
    pub fn into_parts(self) -> (T, MessageId) {
        (self.inner, self.id)
    }
}

/// Helper trait for `WithId` struct interoperability.
pub trait WrapWithId: Sized {
    /// Creates a new instance of `WithId` with the given inner and message ID.
    fn with_id(self, id: MessageId) -> WithId<Self> {
        WithId::new(self, id)
    }

    /// Wraps the given inner value with a message ID
    /// and converts it to the specified type.
    fn with_id_into<T: From<WithId<Self>>>(self, id: MessageId) -> T {
        T::from(self.with_id(id))
    }
}

/// Wrapper for a message along with its source actor ID.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Encode,
    Decode,
    MaxEncodedLen,
    TypeInfo,
    derive_more::Deref,
    derive_more::DerefMut,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct WithSource<T> {
    #[deref]
    #[deref_mut]
    inner: T,

    source: ActorId,
}

impl<T> WithSource<T> {
    /// Creates a new instance of `WithDestination` with the given inner value
    /// and source actor ID.
    pub const fn new(inner: T, source: ActorId) -> Self {
        Self { inner, source }
    }

    /// Returns the source actor ID of the message.
    pub fn source(&self) -> ActorId {
        self.source
    }

    /// Converts the inner type to another type using the `From` trait.
    pub fn convert<U: From<T>>(self) -> WithSource<U> {
        WithSource::new(self.inner.into(), self.source)
    }

    /// Decomposes `self` into the inner data.
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Decomposes `self` into the inner data and the source actor ID.
    pub fn into_parts(self) -> (T, ActorId) {
        (self.inner, self.source)
    }
}

/// Helper trait for `WithSource` struct interoperability.
pub trait WrapWithSource: Sized {
    /// Creates a new instance of `WithSource` with the given
    /// inner and message source.
    fn with_source(self, source: ActorId) -> WithSource<Self> {
        WithSource::new(self, source)
    }

    /// Wraps the given inner value with a source actor ID
    /// and converts it to the specified type.
    fn with_source_into<T: From<WithSource<Self>>>(self, source: ActorId) -> T {
        T::from(self.with_source(source))
    }
}

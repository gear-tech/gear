// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Gear primitive types.
//!
//! Unlike `gstd`, `gcore::general` provides some minimal implementation for
//! `ActorId` and `MessageId` structs with public access to their internals. It
//! can be used provided that you understand how it works and take security
//! considerations into account.
//!
//! `gstd::primitives` declares its own `ActorId` and `MessageId` structures
//! with more extensive methods for access to their internals (no public
//! access). It is recommended to use for most cases.
//!
//! # Examples
//! ```
//! use gstd::ActorId;
//!
//! let id = ActorId::new([0; 32]);
//! let bytes = id.as_ref();
//! ```

use crate::{
    errors::{ContractError, Result},
    prelude::{convert::TryFrom, String},
};
use codec::{Decode, Encode};
use primitive_types::H256;
use scale_info::TypeInfo;

const BS58_MIN_LEN: usize = 35; // Prefix (1) + ID (32) + Checksum (2)

/// Program (actor) identifier.
///
/// Gear allows users and programs to interact with other users and programs via
/// messages. Source and target program as well as user are represented by
/// 256-bit identifier `ActorId` struct. The source `ActorId` for a message
/// being processed can be obtained using [`msg::source`](crate::msg::source)
/// function. Also, each send function has a target `ActorId` as one of the
/// arguments.
#[derive(
    Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, TypeInfo, Decode, Encode,
)]
pub struct ActorId([u8; 32]);

impl ActorId {
    pub const fn new(arr: [u8; 32]) -> Self {
        Self(arr)
    }

    pub const fn zero() -> Self {
        Self::new([0; 32])
    }

    pub fn is_zero(&self) -> bool {
        self == &Self::zero()
    }

    pub fn from_bs58(address: String) -> Result<Self> {
        let decoded = bs58::decode(address)
            .into_vec()
            .map_err(|_| ContractError::Convert("Unable to decode bs58 address"))?;

        let len = decoded.len();
        if len < BS58_MIN_LEN {
            Err(ContractError::Convert("Wrong address len"))
        } else {
            Self::from_slice(&decoded[len - 34..len - 2])
        }
    }

    pub fn from_slice(slice: &[u8]) -> Result<Self> {
        if slice.len() != 32 {
            return Err(ContractError::Convert("Slice should be 32 length"));
        }

        let mut actor_id: Self = Default::default();
        actor_id.0[..].copy_from_slice(slice);

        Ok(actor_id)
    }
}

impl AsRef<[u8]> for ActorId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsMut<[u8]> for ActorId {
    fn as_mut(&mut self) -> &mut [u8] {
        self.0.as_mut()
    }
}

#[cfg(feature = "debug")]
impl From<u64> for ActorId {
    fn from(v: u64) -> Self {
        let mut arr = [0u8; 32];
        arr[0..8].copy_from_slice(&v.to_le_bytes()[..]);
        Self(arr)
    }
}

impl From<[u8; 32]> for ActorId {
    fn from(arr: [u8; 32]) -> Self {
        Self(arr)
    }
}

impl From<ActorId> for [u8; 32] {
    fn from(other: ActorId) -> Self {
        other.0
    }
}

impl From<H256> for ActorId {
    fn from(h256: H256) -> Self {
        Self::new(h256.to_fixed_bytes())
    }
}

impl From<gcore::ActorId> for ActorId {
    fn from(other: gcore::ActorId) -> Self {
        Self(other.0)
    }
}

impl From<ActorId> for gcore::ActorId {
    fn from(other: ActorId) -> Self {
        Self(other.0)
    }
}

impl TryFrom<&[u8]> for ActorId {
    type Error = ContractError;

    fn try_from(slice: &[u8]) -> Result<Self> {
        Self::from_slice(slice)
    }
}

/// Message identifier.
///
/// Gear allows users and programs to interact with other users and programs via
/// messages. Each message has its own unique 256-bit id. This id is represented
/// via the `MessageId` struct. The message identifier can be obtained for the
/// currently processed message using the [`msg::id`](crate::msg::id) function.
/// Also, each send and reply functions return a message identifier.
#[derive(
    Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, TypeInfo, Decode, Encode,
)]
pub struct MessageId([u8; 32]);

#[cfg(feature = "debug")]
impl MessageId {
    pub const fn new(arr: [u8; 32]) -> Self {
        Self(arr)
    }
}

impl AsRef<[u8]> for MessageId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl From<MessageId> for gcore::MessageId {
    fn from(other: MessageId) -> Self {
        Self(other.0)
    }
}

impl From<gcore::MessageId> for MessageId {
    fn from(other: gcore::MessageId) -> Self {
        Self(other.0)
    }
}

#[derive(
    Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, TypeInfo, Decode, Encode,
)]
pub struct CodeId([u8; 32]);

impl CodeId {
    pub const fn new(arr: [u8; 32]) -> Self {
        Self(arr)
    }

    pub fn from_slice(slice: &[u8]) -> Result<Self> {
        if slice.len() != 32 {
            return Err(ContractError::Convert("Slice should be 32 length"));
        }

        let mut ret: Self = Default::default();
        ret.0.as_mut().copy_from_slice(slice);

        Ok(ret)
    }
}

impl AsRef<[u8]> for CodeId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsMut<[u8]> for CodeId {
    fn as_mut(&mut self) -> &mut [u8] {
        self.0.as_mut()
    }
}

impl From<[u8; 32]> for CodeId {
    fn from(arr: [u8; 32]) -> Self {
        Self(arr)
    }
}

impl From<CodeId> for [u8; 32] {
    fn from(other: CodeId) -> Self {
        other.0
    }
}

impl From<H256> for CodeId {
    fn from(h256: H256) -> Self {
        Self::new(h256.to_fixed_bytes())
    }
}

impl From<gcore::CodeId> for CodeId {
    fn from(other: gcore::CodeId) -> Self {
        Self(other.0)
    }
}

impl From<CodeId> for gcore::CodeId {
    fn from(other: CodeId) -> Self {
        Self(other.0)
    }
}

impl TryFrom<&[u8]> for CodeId {
    type Error = ContractError;

    fn try_from(slice: &[u8]) -> Result<Self> {
        Self::from_slice(slice)
    }
}

/// Reservation identifier.
///
/// The ID is used to get reserve and unreserve gas.
///
/// # Examples
///
/// ```
/// use gstd::ReservationId;
///
/// static mut RESERVED: Option<ReservationId> = None;
///
/// unsafe extern "C" fn init() {
///     RESERVED = Some(ReservationId::reserve(50_000_000, 7));
/// }
///
/// unsafe extern "C" fn handle() {
///     let reservation_id = RESERVED.take().expect("create in init()");
///     reservation_id.unreserve();
/// }
/// ```
#[derive(Debug, Hash, Ord, PartialEq, PartialOrd, Eq, TypeInfo, Decode, Encode)]
pub struct ReservationId([u8; 32]);

impl ReservationId {
    pub fn reserve(amount: u64, duration: u32) -> Self {
        gcore::exec::reserve_gas(amount, duration).into()
    }

    pub fn unreserve(self) {
        gcore::exec::unreserve_gas(self.into())
    }
}

impl From<gcore::ReservationId> for ReservationId {
    fn from(id: gcore::ReservationId) -> Self {
        Self(id.0)
    }
}

impl From<ReservationId> for gcore::ReservationId {
    fn from(id: ReservationId) -> Self {
        gcore::ReservationId(id.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_id() {
        let id = ActorId::zero();
        assert_eq!(id.0, [0; 32]);
        assert!(id.is_zero());
    }
}

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
//! is usable provided that you understand how it works and
//! consider security factors.
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
    errors::{ContractError, IntoContractResult, Result},
    prelude::{convert::TryFrom, String},
};
use primitive_types::H256;
use scale_info::{
    scale::{self, Decode, Encode},
    TypeInfo,
};

const BS58_MIN_LEN: usize = 35; // Prefix (1) + ID (32) + Checksum (2)

/// Program (actor) identifier.
///
/// Gear allows user and program interactions via messages.
/// Source and target program as well as user are represented by
/// 256-bit identifier `ActorId` struct. The source `ActorId` for a message
/// being processed can be obtained using [`msg::source`](crate::msg::source)
/// function. Also, each send function has a target `ActorId` as one of the
/// arguments.
#[derive(
    Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, TypeInfo, Decode, Encode,
)]
#[codec(crate = scale)]
pub struct ActorId([u8; 32]);

impl ActorId {
    /// Create a new `ActorId` from a 32-byte array.
    pub const fn new(arr: [u8; 32]) -> Self {
        Self(arr)
    }

    /// Create a new zero `ActorId`.
    pub const fn zero() -> Self {
        Self::new([0; 32])
    }

    /// Check whether `ActorId` is zero.
    pub fn is_zero(&self) -> bool {
        self == &Self::zero()
    }

    /// Create a new `ActorId` from the Base58 string.
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

    /// Create a new `ActorId` from a byte slice.
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
/// Gear allows users and program interactions via messages.
/// Each message has its own unique 256-bit id. This id is represented
/// via the `MessageId` struct. The message identifier can be obtained for the
/// currently processed message using the [`msg::id`](crate::msg::id) function.
/// Also, each send and reply functions return a message identifier.
#[derive(
    Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, TypeInfo, Decode, Encode,
)]
#[codec(crate = scale)]
pub struct MessageId([u8; 32]);

impl MessageId {
    /// Create a new `MessageId` from a 32-byte array.
    pub const fn new(arr: [u8; 32]) -> Self {
        Self(arr)
    }

    /// Create a new zero `MessageId`.
    pub const fn zero() -> Self {
        Self([0; 32])
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

impl From<[u8; 32]> for MessageId {
    fn from(arr: [u8; 32]) -> Self {
        Self(arr)
    }
}

impl From<MessageId> for [u8; 32] {
    fn from(other: MessageId) -> Self {
        other.0
    }
}

impl From<H256> for MessageId {
    fn from(h256: H256) -> Self {
        MessageId(h256.to_fixed_bytes())
    }
}

/// Code identifier.
///
/// This identifier can be obtained as a result of executing the
/// `gear.uploadCode` extrinsic. Actually, the code identifier is the Blake2
/// hash of the Wasm binary code blob.
///
/// Code identifier is required when creating programs from programs (see
/// [`prog`](crate::prog) module for details).
#[derive(
    Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, TypeInfo, Decode, Encode,
)]
#[codec(crate = scale)]
pub struct CodeId([u8; 32]);

impl CodeId {
    /// Create a new `CodeId` from a 32-byte array.
    pub const fn new(arr: [u8; 32]) -> Self {
        Self(arr)
    }

    /// Create a new `CodeId` from a byte slice.
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
/// The identifier is used to reserve and unreserve gas amount
/// for program execution later.
///
/// # Examples
///
/// ```
/// use gstd::ReservationId;
///
/// static mut RESERVED: Option<ReservationId> = None;
///
/// extern "C" fn init() {
///     let reservation_id = ReservationId::reserve(50_000_000, 7).expect("Unable to reserve");
///     unsafe { RESERVED = Some(reservation_id) };
/// }
///
/// extern "C" fn handle() {
///     let reservation_id = unsafe { RESERVED.take().expect("Empty `RESERVED`") };
///     reservation_id.unreserve();
/// }
/// ```
#[derive(Clone, Copy, Debug, Hash, Ord, PartialEq, PartialOrd, Eq, TypeInfo, Decode, Encode)]
#[codec(crate = scale)]
pub struct ReservationId([u8; 32]);

impl ReservationId {
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
    /// use gstd::{msg, ReservationId};
    ///
    /// #[no_mangle]
    /// extern "C" fn handle() {
    ///     let reservation_id = ReservationId::reserve(50_000_000, 1).expect("Unable to reserve");
    ///     msg::reply_bytes_from_reservation(reservation_id.clone(), b"PONG", 0)
    ///         .expect("Unable to reply");
    ///     let reservation_left = reservation_id.unreserve().expect("Unable to unreserve");
    /// }
    /// ```
    pub fn reserve(amount: u64, duration: u32) -> Result<Self> {
        gcore::exec::reserve_gas(amount, duration).into_contract_result()
    }

    /// Unreserve unused gas from the reservation.
    ///
    /// If successful, it returns the reserved amount of gas.
    pub fn unreserve(self) -> Result<u64> {
        gcore::exec::unreserve_gas(self.into()).into_contract_result()
    }
}

impl AsRef<[u8]> for ReservationId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
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

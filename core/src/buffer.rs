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

//! Vector with limited len realization.

use core::{
    convert::TryFrom,
    fmt::{Debug, Display},
};

use alloc::sync::Arc;

use parity_scale_codec::{Compact, MaxEncodedLen};
use scale_info::{
    TypeInfo,
    scale::{Decode, Encode},
};

use crate::limited::{LimitedStr, LimitedVec, LimitedVecError};

/// Max memory size, which runtime can allocate at once.
/// Substrate allocator restrict allocations bigger then 512 wasm pages at once.
/// See more information about:
/// https://github.com/paritytech/substrate/blob/cc4d5cc8654d280f03a13421669ba03632e14aa7/client/allocator/src/freeing_bump.rs#L136-L149
/// https://github.com/paritytech/substrate/blob/cc4d5cc8654d280f03a13421669ba03632e14aa7/primitives/core/src/lib.rs#L385-L388
const RUNTIME_MAX_ALLOC_SIZE: usize = 512 * 0x10000;

/// Take half from [RUNTIME_MAX_ALLOC_SIZE] in order to avoid problems with capacity overflow.
const RUNTIME_MAX_BUFF_SIZE: usize = RUNTIME_MAX_ALLOC_SIZE / 2;

/// Wrapper for payload slice.
pub struct PayloadSlice {
    /// Start of the slice.
    start: usize,
    /// End of the slice.
    end: usize,
    /// Payload
    payload: Arc<Payload>,
}

impl PayloadSlice {
    /// Try to create a new PayloadSlice.
    pub fn try_new(start: u32, end: u32, payload: Arc<Payload>) -> Option<Self> {
        // Check if start and end are within the bounds of the payload
        if start > end || end > payload.len_u32() {
            return None;
        }

        Some(Self {
            start: start as usize,
            end: end as usize,
            payload,
        })
    }

    /// Get slice of the payload.
    pub fn slice(&self) -> &[u8] {
        &self.payload[self.start..self.end]
    }
}

/// Buffer which size cannot be bigger then max allowed allocation size in runtime.
pub type RuntimeBuffer = LimitedVec<u8, RUNTIME_MAX_BUFF_SIZE>;

/// Max payload size which one message can have (8 MiB).
pub const MAX_PAYLOAD_SIZE: usize = 8 * 1024 * 1024;

// **WARNING**: do not remove this check
const _: () = assert!(MAX_PAYLOAD_SIZE <= u32::MAX as usize);

/// Payload type for message.
pub type Payload = LimitedVec<u8, MAX_PAYLOAD_SIZE>;

impl Payload {
    /// Get payload length as u32.
    pub fn len_u32(&self) -> u32 {
        // Safe, cause it's guarantied: `MAX_PAYLOAD_SIZE` <= u32::MAX
        self.len() as u32
    }
}

impl MaxEncodedLen for Payload {
    fn max_encoded_len() -> usize {
        Compact::<u32>::max_encoded_len() + MAX_PAYLOAD_SIZE
    }
}

/// Panic buffer which size cannot be bigger then max allowed payload size.
#[derive(
    Clone,
    Default,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Decode,
    Encode,
    TypeInfo,
    derive_more::From,
    derive_more::Into,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct PanicBuffer(Payload);

impl PanicBuffer {
    /// Returns ref to the internal data.
    pub fn inner(&self) -> &Payload {
        &self.0
    }

    fn to_limited_str(&self) -> Option<LimitedStr<'_>> {
        let s = core::str::from_utf8(&self.0).ok()?;
        LimitedStr::try_from(s).ok()
    }
}

impl From<LimitedStr<'_>> for PanicBuffer {
    fn from(value: LimitedStr) -> Self {
        const _: () = assert!(<LimitedStr<'_>>::MAX_LEN <= MAX_PAYLOAD_SIZE);
        Payload::try_from(value.into_inner().into_owned().into_bytes())
            .map(Self)
            .unwrap_or_else(|LimitedVecError| {
                unreachable!("`LimitedStr` is always smaller than maximum payload size",)
            })
    }
}

impl Display for PanicBuffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(s) = self.to_limited_str() {
            Display::fmt(&s, f)
        } else {
            Display::fmt(&self.0, f)
        }
    }
}

impl Debug for PanicBuffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(s) = self.to_limited_str() {
            Debug::fmt(s.as_str(), f)
        } else {
            Debug::fmt(&self.0, f)
        }
    }
}

#[cfg(test)]
mod test {
    use super::{PanicBuffer, Payload};
    use alloc::format;
    use core::convert::TryFrom;

    fn panic_buf(bytes: &[u8]) -> PanicBuffer {
        Payload::try_from(bytes).map(PanicBuffer).unwrap()
    }

    #[test]
    fn panic_buffer_debug() {
        let buf = panic_buf(b"Hello, world!");
        assert_eq!(format!("{buf:?}"), r#""Hello, world!""#);

        let buf = panic_buf(b"\xE0\x80\x80");
        assert_eq!(format!("{buf:?}"), "0xe08080");
    }

    #[test]
    fn panic_buffer_display() {
        let buf = panic_buf(b"Hello, world!");
        assert_eq!(format!("{buf}"), "Hello, world!");

        let buf = panic_buf(b"\xE0\x80\x80");
        assert_eq!(format!("{buf}"), "0xe08080");
    }
}

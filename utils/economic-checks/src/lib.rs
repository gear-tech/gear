// This file is part of Gear.

// Copyright (C) 202 Gear Technologies Inc.
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

// #![cfg_attr(not(feature = "std"), no_std)]

// extern crate alloc;

use arbitrary::{Arbitrary, Error, Result, Unstructured};
pub use checks::*;

pub mod util;

pub mod checks;

#[derive(Debug, Clone)]
pub struct Params {
    depth: u16,
    intrinsic_value: u64,
    gas_limit: u64,
}

impl<'a> Arbitrary<'a> for Params {
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
        if u.len() < 18 {
            return Err(Error::NotEnoughData);
        }

        let mut entropy = [0u8; 18];
        u.fill_buffer(&mut entropy)?;

        let mut buf = [0u8; 2];
        buf.copy_from_slice(&entropy[0..2]);
        let depth: u16 = u16::from_le_bytes(buf) >> 6; // [0..1024]

        let mut buf = [0u8; 8];
        buf.copy_from_slice(&entropy[2..10]);
        let intrinsic_value: u64 = 100 + (u64::from_le_bytes(buf) >> 32); // [100.. ~4*10^9]

        let mut buf = [0u8; 8];
        buf.copy_from_slice(&entropy[10..18]);
        let gas_limit: u64 = 10_000_000_u64 + (u64::from_le_bytes(buf) >> 24); // [10^7.. ~10^12]

        Ok(Params {
            depth,
            intrinsic_value,
            gas_limit,
        })
    }

    #[inline]
    fn size_hint(_depth: usize) -> (usize, Option<usize>) {
        (18, Some(18))
    }
}

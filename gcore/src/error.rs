// This file is part of Gear.
//
// Copyright (C) 2022 Gear Technologies Inc.
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

pub use gear_core_errors::*;

pub type Result<T, E = ExtError> = core::result::Result<T, E>;

#[cfg(feature = "codec")]
mod sys {
    use crate::error::SyscallError;

    extern "C" {
        pub fn gr_error(buffer_ptr: *mut u8) -> SyscallError;
    }
}

#[must_use]
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SyscallError(u32);

impl SyscallError {
    pub fn into_result(self) -> Result<()> {
        if self.0 != 0 {
            return Err(self.get_err()?);
        }

        Ok(())
    }

    // We get an error using `gr_error` syscall which expects
    // the error occurred earlier in another syscall or you'll get trap.
    // Error decoding is expected to be successful because we use
    // SCALE codec crate of same versions (at least major ones)
    // to encode and to decode error so error representation stays same.
    // If `len` argument is less than actual encoded error length you'll get trap.
    #[cfg(feature = "codec")]
    fn get_err(self) -> Result<ExtError> {
        use alloc::vec;
        use codec::Decode;

        let mut data = vec![0; self.0 as usize];

        unsafe { sys::gr_error(data.as_mut_ptr()).into_result()? }

        Ok(ExtError::decode(&mut data.as_slice()).unwrap_or(ExtError::Decode))
    }

    #[cfg(not(feature = "codec"))]
    fn get_err(self) -> Result<ExtError> {
        Ok(ExtError::Some)
    }
}

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

pub use gear_core_errors::{ExtError, MemoryError, MessageError, TerminationReason};

#[cfg(feature = "codec")]
mod sys {
    extern "C" {
        pub fn gr_error(data: *mut u8);
    }
}

#[must_use]
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SyscallError {
    len: u32,
}

impl SyscallError {
    pub fn into_result(self) -> Result<(), ExtError> {
        if self.len == 0 {
            Ok(())
        } else if cfg!(feature = "codec") {
            unsafe {
                use alloc::vec;
                use codec::Decode;

                let mut data = vec![0; self.len as usize];
                sys::gr_error(data.as_mut_ptr());
                Err(ExtError::decode(&mut data.as_slice()).expect("error decoded successfully"))
            }
        } else {
            Err(ExtError::Unknown)
        }
    }
}

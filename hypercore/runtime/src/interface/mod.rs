// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

#[path = "code.rs"]
pub(crate) mod code_ri;

#[path = "logging.rs"]
pub(crate) mod logging_ri;

pub(crate) mod utils {
    pub fn repr_ri_slice(slice: impl AsRef<[u8]>) -> i64 {
        let slice = slice.as_ref();

        let ptr = slice.as_ptr() as i32;
        let len = slice.len() as i32;

        let mut res = [0u8; 8];
        res[..4].copy_from_slice(&ptr.to_le_bytes());
        res[4..].copy_from_slice(&len.to_le_bytes());

        i64::from_le_bytes(res)
    }
}

// TODO: remove me
pub(crate) mod program_ri {
    use gprimitives::ActorId as ProgramId;

    mod sys {
        extern "C" {
            pub fn program_id(program_id_ptr: *mut [u8; 32]);
        }
    }

    pub fn program_id() -> ProgramId {
        let mut buffer = [0; 32];

        unsafe {
            sys::program_id(buffer.as_mut_ptr() as _);
        }

        buffer.into()
    }
}

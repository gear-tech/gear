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

use super::utils;
use alloc::{vec, vec::Vec};
use core::fmt::{self, Write};
use gprimitives::CodeId;
use log::{Level, LevelFilter, Metadata, Record};

mod sys {
    extern "C" {
        pub fn code_load_v1(buffer: *mut u8);
    }
}

pub fn load(len: usize) -> Vec<u8> {
    let mut buffer = vec![0; len];

    unsafe { sys::code_load_v1(buffer.as_mut_ptr() as _) }

    buffer
}

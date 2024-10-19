// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

//! Gear core.
//!
//! This library provides a runner for dealing with multiple little programs exchanging messages in a deterministic manner.
//! To be used primary in Gear Substrate node implementation, but it is not limited to that.
#![no_std]
#![warn(missing_docs)]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]

extern crate alloc;

pub mod buffer;
pub mod code;
pub mod costs;
pub mod env;
pub mod env_vars;
pub mod gas;
pub mod gas_metering;
pub mod ids;
pub mod memory;
pub mod message;
pub mod pages;
pub mod percent;
pub mod program;
pub mod reservation;
pub mod str;
pub mod tasks;

// This allows all casts from u32 into usize be safe.
const _: () = assert!(size_of::<u32>() <= size_of::<usize>());

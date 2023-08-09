// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

//! Functions and helpers for creating programs from programs.
//!
//! Any program being an actor, can not only process incoming messages and send
//! outcoming messages to other actors but also create new actors. This feature
//! can be useful when implementing the factory pattern, as a single
//! actor can produce multiple derived actors with different input data.
//!
//! Firstly you need to upload a Wasm code of the future program(s) by calling
//! `gear.uploadCode` extrinsic to obtain the corresponding
//! [`CodeId`](crate::CodeId).
//!
//! You must also provide a unique byte sequence to create multiple program
//! instances from the same code. This sequence is often referenced as _salt_.
//! [`ProgramGenerator`] allows generating of salt automatically.
//!
//! The newly created program should be initialized using a corresponding
//! payload; therefore, you must provide it when calling any `create_program_*`
//! function.

mod generator;
pub use generator::ProgramGenerator;

mod basic;
pub use basic::*;

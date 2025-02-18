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

//! Necessary traits definitions.
//! Derived from the implementation in the [`plonky2`](https://crates.io/crates/plonky2) crate.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::{
    field::{goldilocks_field::GoldilocksField, types::PrimeField64},
    hash::poseidon::Poseidon,
};

/// A prime order field with the features we need to use it as a base field in our argument system.
pub trait RichField: PrimeField64 + Poseidon {}

impl RichField for GoldilocksField {}

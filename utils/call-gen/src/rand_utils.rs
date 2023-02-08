// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

//! Randomization utilities

use dyn_clonable::clonable;
use rand::{Rng, RngCore, SeedableRng};

/// Trait that aggregates `RngCore` and `Clone` traits
///
/// Auto implemented for the implementors of the aggregated traits.
#[clonable]
pub trait CallGenRngCore: RngCore + Clone {}
impl<T: RngCore + Clone> CallGenRngCore for T {}

/// Trait that aggregates `Rng`, `SeedableRng` and `Clone`.
///
/// Auto implemented for the implementors of the aggregated traits.
pub trait CallGenRng: Rng + SeedableRng + 'static + Clone {}
impl<T: Rng + SeedableRng + 'static + Clone> CallGenRng for T {}

// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Randomization utilities

use dyn_clonable::clonable;
use rand::{Rng, RngCore, SeedableRng};

/// Trait that aggregates `RngCore` and `Clone` traits
///
/// Auto implemented for the implementors of the aggregated traits.
#[clonable]
pub trait CallGenRngCore: RngCore + Clone + Send {}
impl<T: RngCore + Clone + Send> CallGenRngCore for T {}

/// Trait that aggregates `Rng`, `SeedableRng` and `Clone`.
///
/// Auto implemented for the implementors of the aggregated traits.
pub trait CallGenRng: Rng + SeedableRng + 'static + Clone + Send {}
impl<T: Rng + SeedableRng + 'static + Clone + Send> CallGenRng for T {}

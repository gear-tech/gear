// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Gear's storage API module.

// Private modules declaration.
mod complex;
mod complicated;
mod primitives;

// Public exports from private storage modules.
pub use complex::*;
pub use complicated::*;
pub use primitives::*;

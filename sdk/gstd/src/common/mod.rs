// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Common modules for each Gear program.

pub mod errors;
mod handlers;
#[cfg(not(feature = "ethexe"))]
pub mod primitives_ext;

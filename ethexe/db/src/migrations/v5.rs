// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Schema version 5 anchor.
//!
//! Holds the [`VERSION`] constant referenced by
//! [`super::OLDEST_SUPPORTED_VERSION`] and [`super::LATEST_VERSION`]. No
//! migration function lives here — when the next schema bump lands, the
//! new module (e.g. `v6`) gets a `migration_from_v5` and the
//! [`super::MIGRATIONS`] slice grows.

pub const VERSION: u32 = 5;

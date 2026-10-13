// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

/// Base error code for RPC modules.
pub mod base {
    pub const AUTHOR: i32 = 1000;
    pub const SYSTEM: i32 = 2000;
    pub const CHAIN: i32 = 3000;
    pub const STATE: i32 = 4000;
    pub const OFFCHAIN: i32 = 5000;
    pub const DEV: i32 = 6000;
    pub const STATEMENT: i32 = 7000;
    pub const MIXNET: i32 = 8000;
}

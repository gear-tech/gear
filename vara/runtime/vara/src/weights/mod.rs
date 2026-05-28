// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! A list of the different weight modules for our runtime.

// Dead code is allowed for the weights module due to unused copies of the `WeightInfo` trait.
#![allow(dead_code)]

pub mod frame_system;
pub mod pallet_balances;
pub mod pallet_timestamp;
pub mod pallet_utility;

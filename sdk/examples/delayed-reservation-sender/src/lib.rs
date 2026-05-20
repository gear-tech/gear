// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![no_std]

use gstd::codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

pub const SENDING_EXPECT: &str = "Failed to send delayed message from reservation";

#[derive(Encode, Decode, Debug, Clone, Copy)]
#[codec(crate = gstd::codec)]
pub enum ReservationSendingShowcase {
    ToSourceInPlace {
        reservation_amount: u64,
        reservation_delay: u32,
        sending_delay: u32,
    },
    ToSourceAfterWait {
        reservation_amount: u64,
        reservation_delay: u32,
        wait_for: u32,
        sending_delay: u32,
    },
}

#[cfg(not(feature = "std"))]
mod wasm;

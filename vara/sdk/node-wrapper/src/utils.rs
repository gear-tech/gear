// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Picking random ports
use rand::{Rng, rngs::OsRng};
use std::{net::TcpListener, ops::Range};

/// localhost addr
pub const LOCALHOST: &str = "127.0.0.1";
const PORT_RANGE: Range<u16> = 15000..25000;

/// Pick a random port
pub fn pick() -> u16 {
    loop {
        let port = OsRng.gen_range(PORT_RANGE);
        if TcpListener::bind(format!("{LOCALHOST}:{port}")).is_ok() {
            return port;
        }
    }
}

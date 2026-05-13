// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(not(feature = "std"))]
pub(crate) mod wasm {
    pub fn init(addr: gstd::ActorId) -> ! {
        let _ = gstd::msg::send_bytes(addr, b"PING", 0).unwrap();
        gstd::exec::wait_up_to(100)
    }

    pub fn handle_reply() -> ! {
        gstd::exec::exit(gstd::msg::source())
    }
}

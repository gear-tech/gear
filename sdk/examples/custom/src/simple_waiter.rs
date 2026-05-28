// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(not(feature = "std"))]
pub(crate) mod wasm {
    use gstd::{exec, msg, prelude::*};

    #[derive(Default)]
    pub(crate) struct State {
        triggered: bool,
    }

    pub(crate) fn init() -> State {
        Default::default()
    }

    pub(crate) fn handle(state: &mut State) {
        if !state.triggered {
            state.triggered = true;
            exec::wait_for(20);
        }

        msg::send_bytes(msg::source(), b"hello", 0).unwrap();
    }
}

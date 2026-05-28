// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(not(feature = "std"))]
pub(crate) mod wasm {
    use gsys::{ErrorWithHash, HashWithValue};

    pub(crate) struct State;

    pub(crate) fn init() -> State {
        // Code below is copied and simplified from `gcore::msg::send`.
        let pid_value = HashWithValue {
            hash: [0; 32],
            value: 0,
        };

        let mut res: ErrorWithHash = Default::default();

        // u32::MAX ptr + 42 len of the payload triggers error of payload read.
        unsafe {
            gsys::gr_send(
                pid_value.as_ptr(),
                u32::MAX as *const u8,
                42,
                0,
                res.as_mut_ptr(),
            )
        };

        assert!(res.error_code != 0);

        State
    }
}

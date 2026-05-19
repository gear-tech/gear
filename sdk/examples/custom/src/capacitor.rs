// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(not(feature = "std"))]
pub(crate) mod wasm {
    use gstd::{String, Vec, debug, msg, prelude::*, str::FromStr};

    #[derive(Default)]
    pub(crate) struct State {
        charge: u32,
        limit: u32,
        discharge_history: Vec<u32>,
    }

    pub(crate) fn init(payload: String) -> State {
        let limit = u32::from_str(payload.as_ref()).expect("Invalid number");
        debug!("Init capacitor with limit capacity {limit}, {payload}");
        State {
            charge: 0,
            limit,
            discharge_history: Vec::new(),
        }
    }

    pub(crate) fn handle(state: &mut State) {
        let new_msg = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
            .expect("Invalid message: should be utf-8");
        let to_add = u32::from_str(new_msg.as_ref()).expect("Invalid number");

        state.charge += to_add;
        debug!(
            "Charge capacitor with {to_add}, new charge {}",
            state.charge
        );
        if state.charge >= state.limit {
            debug!("Discharge #{} due to limit {}", state.charge, state.limit);
            msg::send_bytes(msg::source(), format!("Discharged: {}", state.charge), 0).unwrap();
            state.discharge_history.push(state.charge);
            state.charge = 0;
        }
    }
}

// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(not(feature = "std"))]
pub(crate) mod wasm {
    use gstd::{ReservationId, msg, prelude::*};

    #[derive(Default)]
    pub(crate) struct State {
        reservation_id: Option<ReservationId>,
    }

    pub(crate) fn handle(state: &mut State) {
        if let Some(id) = state.reservation_id.take() {
            msg::send_bytes_from_reservation(id, msg::source(), b"hello", 0)
                .expect("Unable to send from reservation");
        } else {
            state.reservation_id =
                Some(ReservationId::reserve(100_000_000, 10).expect("Unable to reserve"));
        }
    }
}

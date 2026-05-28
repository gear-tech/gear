// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Bridge builtin storage manager.

use std::thread::LocalKey;

use crate::state::WithOverlay;
use gprimitives::U256;

thread_local! {
    pub(super) static BRIDGE_MESSAGE_NONCE: WithOverlay<U256> = Default::default();
}

fn storage() -> &'static LocalKey<WithOverlay<U256>> {
    &BRIDGE_MESSAGE_NONCE
}

pub(crate) struct BridgeBuiltinStorage;

impl BridgeBuiltinStorage {
    /// Get the current message nonce.
    pub(crate) fn fetch_nonce() -> U256 {
        storage().with(|nonce| {
            let mut data = nonce.data_mut();
            let ret = *data;
            *data = data.saturating_add(U256::one());

            ret
        })
    }

    pub(crate) fn clear() {
        storage().with(|nonce| {
            *nonce.data_mut() = U256::zero();
        });
    }
}

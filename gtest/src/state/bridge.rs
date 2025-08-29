// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

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
        todo!("todo [sab]");
    }
}

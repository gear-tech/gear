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

//! Nonce manager.

use std::{cell::Cell, thread::LocalKey};

thread_local! {
    /// Definition of the storage value storing message nonce.
    pub(super) static MSG_NONCE: Cell<u64> = const { Cell::new(1) };
    /// Definition of the storage value storing id nonce.
    pub(super) static ID_NONCE: Cell<u64> = const { Cell::new(1) };
}

fn msg_nonce_storage() -> &'static LocalKey<Cell<u64>> {
    if super::overlay_enabled() {
        &super::MSG_NONCE_OVERLAY
    } else {
        &MSG_NONCE
    }
}

fn id_nonce_storage() -> &'static LocalKey<Cell<u64>> {
    if super::overlay_enabled() {
        &super::ID_NONCE_OVERLAY
    } else {
        &ID_NONCE
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct NonceManager;

impl NonceManager {
    pub(crate) fn fetch_inc_message_nonce(&self) -> u64 {
        msg_nonce_storage().with(|nonce| {
            let value = nonce.get();
            nonce.set(value + 1);
            value
        })
    }

    pub(crate) fn id_nonce(&self) -> u64 {
        id_nonce_storage().with(|nonce| nonce.get())
    }

    pub(crate) fn inc_id_nonce(&self) {
        id_nonce_storage().with(|nonce| {
            let value = nonce.get();
            nonce.set(value + 1);
        });
    }
}

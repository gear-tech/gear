// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use gear_core::storage::Storage;
use gear_runner::runner::{Config, Runner};

use crate::ext::*;

const MEMORY_KEY_PREFIX: &'static [u8] = b"g::memory";

pub type ExtRunner = Runner<ExtAllocationStorage, ExtMessageQueue, ExtProgramStorage>;

fn memory() -> Vec<u8> {
    sp_externalities::with_externalities(|ext| ext.storage(MEMORY_KEY_PREFIX))
        .expect("Called outside of externalities context")
        .unwrap_or_default()
}

pub fn set_memory(data: Vec<u8>) {
    sp_externalities::with_externalities(|ext| { ext.set_storage(MEMORY_KEY_PREFIX.to_vec(), data); })
        .expect("Called outside of externalities context");
}

pub fn new() -> ExtRunner {
    Runner::new(
        &Config::default(),
        Storage {
            allocation_storage: ExtAllocationStorage,
            message_queue: ExtMessageQueue::default(),
            program_storage: ExtProgramStorage,
        },
        &memory(),
    )
}

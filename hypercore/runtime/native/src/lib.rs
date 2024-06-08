// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

//! Native runtime implementation.

pub use hypercore_runtime_common::RuntimeInterface;

use gear_core::pages::GearPage;
use gear_core_processor::configs::BlockInfo;
use gear_lazy_pages::LazyPagesVersion;
use gear_lazy_pages_common::LazyPagesInitContext;
use gear_lazy_pages_native_interface::LazyPagesNative;
use gprimitives::H256;
use hypercore_runtime_common::state::Storage;
use pages_storage::PagesStorage;
use std::collections::BTreeMap;

pub use hypercore_runtime_common::{self, process_program, state};

mod pages_storage;

pub struct NativeRuntimeInterface<'a, S: Storage> {
    storage: &'a S,
}

impl<'a, S: Storage> NativeRuntimeInterface<'a, S> {
    pub fn new(storage: &'a S) -> Self {
        Self { storage }
    }
}

impl<S: Storage> RuntimeInterface<S> for NativeRuntimeInterface<'_, S> {
    type LazyPages = LazyPagesNative;

    fn block_info(&self) -> BlockInfo {
        BlockInfo::default() // TODO
    }

    fn init_lazy_pages(&self, pages_map: BTreeMap<GearPage, H256>) {
        let pages_storage = PagesStorage {
            storage: self.storage.clone_boxed(),
            memory_map: pages_map,
        };
        gear_lazy_pages::init(
            LazyPagesVersion::Version1,
            LazyPagesInitContext::new(Default::default()),
            pages_storage,
        )
        .expect("Failed to init lazy-pages");
    }

    fn random_data(&self) -> (Vec<u8>, u32) {
        (vec![0; 32], 0) // TODO
    }

    fn storage(&self) -> &S {
        self.storage
    }
}

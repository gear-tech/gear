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

//! Pages storage for gpu to use in gear-lazy-pages.

use gear_core::{ids::ProgramId, pages::GearPage};
use gear_lazy_pages::LazyPagesStorage;
use gprimitives::H256;
use hypercore_db::CASDatabase;
use parity_scale_codec::{Decode, DecodeAll};
use std::{
    collections::BTreeMap,
    fmt::{self, Debug, Formatter},
};

#[derive(Decode)]
struct PageKey {
    _page_storage_prefix: [u8; 32],
    _program_id: ProgramId,
    _memory_infix: u32,
    page: GearPage,
}

pub(crate) struct PagesStorage {
    pub db: Box<dyn CASDatabase>,
    pub memory_map: BTreeMap<GearPage, H256>,
}

impl Debug for PagesStorage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("PagesStorage").finish()
    }
}

impl LazyPagesStorage for PagesStorage {
    fn page_exists(&self, mut key: &[u8]) -> bool {
        let PageKey { page, .. } = PageKey::decode_all(&mut key).expect("Invalid key");
        self.memory_map.contains_key(&page)
    }

    fn load_page(&mut self, mut key: &[u8], buffer: &mut [u8]) -> Option<u32> {
        let PageKey { page, .. } = PageKey::decode_all(&mut key).expect("Invalid key");
        self.memory_map.get(&page).map(|hash| {
            let data = self.db.read(hash).expect("Cannot read page from db");
            buffer.copy_from_slice(&data);
            data.len() as u32
        })
    }
}

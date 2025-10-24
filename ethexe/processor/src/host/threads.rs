// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

// TODO: for each panic here place log::error, otherwise it won't be printed.

use core::fmt;
use ethexe_common::db::OnChainStorageRO;
use ethexe_db::Database;
use ethexe_runtime_common::{
    BlockInfo,
    state::{
        ActiveProgram, HashOf, MemoryPages, MemoryPagesRegionInner, Program, ProgramState,
        RegionIdx, Storage,
    },
};
use gear_core::{ids::ActorId, memory::PageBuf, pages::GearPage};
use gear_lazy_pages::LazyPagesStorage;
use gprimitives::H256;
use parity_scale_codec::{Decode, DecodeAll};
use std::{cell::RefCell, collections::BTreeMap};

const UNSET_PANIC: &str = "params should be set before query";
const UNKNOWN_STATE: &str = "state should always be valid (must exist)";

thread_local! {
    static PARAMS: RefCell<Option<ThreadParams>> = const { RefCell::new(None) };
}

pub struct ThreadParams {
    pub db: Database,
    pub block_info: BlockInfo,
    pub state_hash: H256,
    pages_registry_cache: Option<MemoryPages>,
    pages_regions_cache: Option<BTreeMap<RegionIdx, MemoryPagesRegionInner>>,
}

impl fmt::Debug for ThreadParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ThreadParams")
    }
}

impl ThreadParams {
    pub fn get_page_region(
        &mut self,
        page: GearPage,
    ) -> Option<&BTreeMap<GearPage, HashOf<PageBuf>>> {
        let pages_registry = self.pages_registry_cache.get_or_insert_with(|| {
            let ProgramState {
                program: Program::Active(ActiveProgram { pages_hash, .. }),
                ..
            } = self.db.program_state(self.state_hash).expect(UNKNOWN_STATE)
            else {
                unreachable!("program that is currently running can't be inactive");
            };

            pages_hash.query(&self.db).expect(UNKNOWN_STATE)
        });

        let region_idx = MemoryPages::page_region(page);

        let region_hash = pages_registry[region_idx].to_inner()?;

        let pages_regions = self
            .pages_regions_cache
            .get_or_insert_with(Default::default);

        let page_region = pages_regions.entry(region_idx).or_insert_with(|| {
            self.db
                .memory_pages_region(region_hash)
                .expect("Pages region not found")
                .into()
        });

        Some(&*page_region)
    }
}

#[derive(Decode)]
struct PageKey {
    _page_storage_prefix: [u8; 32],
    _program_id: ActorId,
    _memory_infix: u32,
    page: GearPage,
}

impl PageKey {
    fn page_from_buf(mut buf: &[u8]) -> GearPage {
        let PageKey { page, .. } = PageKey::decode_all(&mut buf).expect("Invalid key");
        page
    }
}

pub fn set(db: Database, chain_head: H256, state_hash: H256) {
    let header = db.block_header(chain_head).expect("Block info not found");
    PARAMS.set(Some(ThreadParams {
        db,
        block_info: BlockInfo {
            height: header.height,
            timestamp: header.timestamp,
        },
        state_hash,
        pages_registry_cache: None,
        pages_regions_cache: None,
    }))
}

pub fn update_state_hash(state_hash: H256) {
    PARAMS.with_borrow_mut(|v| {
        let params = v.as_mut().expect(UNSET_PANIC);

        params.state_hash = state_hash;
        params.pages_registry_cache = None;
        params.pages_regions_cache = None;
    })
}

pub fn with_db<T>(f: impl FnOnce(&Database) -> T) -> T {
    PARAMS.with_borrow(|v| {
        let params = v.as_ref().expect(UNSET_PANIC);

        f(&params.db)
    })
}

pub fn chain_head_info() -> BlockInfo {
    PARAMS.with_borrow(|v| {
        let params = v.as_ref().expect(UNSET_PANIC);

        params.block_info
    })
}

pub fn with_params<T>(f: impl FnOnce(&mut ThreadParams) -> T) -> T {
    PARAMS.with_borrow_mut(|v| {
        let params = v.as_mut().expect(UNSET_PANIC);

        f(params)
    })
}

#[derive(Debug)]
pub struct EthexeHostLazyPages;

impl LazyPagesStorage for EthexeHostLazyPages {
    fn load_page(&mut self, key: &[u8], buffer: &mut [u8]) -> Option<u32> {
        with_params(|params| {
            let page = PageKey::page_from_buf(key);

            let page_hash = params.get_page_region(page)?.get(&page).cloned()?;

            let data = params.db.page_data(page_hash).expect("Page not found");

            buffer.copy_from_slice(&data);

            Some(data.len() as u32)
        })
    }

    fn page_exists(&self, key: &[u8]) -> bool {
        with_params(|params| {
            let page = PageKey::page_from_buf(key);

            params
                .get_page_region(page)
                .map(|region| region.contains_key(&page))
                .unwrap_or(false)
        })
    }
}

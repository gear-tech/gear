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

// TODO: for each panic here place log::error, otherwise it won't be printed.

use crate::Database;
use core::fmt;
use ethexe_db::BlockMetaStorage;
use ethexe_runtime_common::{
    state::{ActiveProgram, HashOf, Program, ProgramState, Storage},
    BlockInfo,
};
use gear_core::{ids::ProgramId, memory::PageBuf, pages::GearPage};
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
    pub pages: Option<BTreeMap<GearPage, HashOf<PageBuf>>>,
}

impl fmt::Debug for ThreadParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ThreadParams")
    }
}

impl ThreadParams {
    pub fn pages(&mut self) -> &BTreeMap<GearPage, HashOf<PageBuf>> {
        self.pages.get_or_insert_with(|| {
            let ProgramState {
                program: Program::Active(ActiveProgram { pages_hash, .. }),
                ..
            } = self.db.read_state(self.state_hash).expect(UNKNOWN_STATE)
            else {
                // TODO: consider me.
                panic!("Couldn't get pages hash for inactive program!")
            };

            pages_hash.query(&self.db).expect(UNKNOWN_STATE).into()
        })
    }
}

#[derive(Decode)]
struct PageKey {
    _page_storage_prefix: [u8; 32],
    _program_id: ProgramId,
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
        pages: None,
    }))
}

// TODO: consider Database mutability.
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

// TODO: consider Database mutability.
#[allow(unused)]
pub fn with_db_and_state_hash<T>(f: impl FnOnce(&Database, H256) -> T) -> T {
    PARAMS.with_borrow(|v| {
        let params = v.as_ref().expect(UNSET_PANIC);

        f(&params.db, params.state_hash)
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

            let page_hash = params.pages().get(&page).cloned()?;

            let data = params.db.read_page_data(page_hash).expect("Page not found");

            buffer.copy_from_slice(&data);

            Some(data.len() as u32)
        })
    }

    fn page_exists(&self, key: &[u8]) -> bool {
        with_params(|params| {
            let page = PageKey::page_from_buf(key);

            params.pages().contains_key(&page)
        })
    }
}

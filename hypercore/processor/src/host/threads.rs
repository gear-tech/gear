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

use crate::Database;
use gprimitives::H256;
use std::cell::RefCell;

thread_local! {
    static PARAMS: RefCell<Option<ThreadParams>> = const { RefCell::new(None) };
}

pub struct ThreadParams {
    pub db: Database,
    pub root: H256,
}

const UNSET_PANIC: &str = "params should be set before query";

pub fn set(db: Database, root: H256) {
    PARAMS.set(Some(ThreadParams { db, root }))
}

pub fn replace_root(root: H256) -> H256 {
    PARAMS.with_borrow_mut(|v| {
        let params = v.as_mut().expect(UNSET_PANIC);

        let prev = params.root;
        params.root = root;
        prev
    })
}

// TODO: consider Database mutability.
pub fn with_db<T>(f: impl FnOnce(&Database) -> T) -> T {
    PARAMS.with_borrow(|v| {
        let params = v.as_ref().expect(UNSET_PANIC);

        f(&params.db)
    })
}

// TODO: consider Database mutability.
pub fn with_db_and_root<T>(f: impl FnOnce(&Database, H256) -> T) -> T {
    PARAMS.with_borrow(|v| {
        let params = v.as_ref().expect(UNSET_PANIC);

        f(&params.db, params.root)
    })
}

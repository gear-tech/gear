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

//! Database library for hypercore.

use gear_core::ids;
use gprimitives::H256;

mod mem;
mod rocks;

pub use mem::MemDb;
pub use rocks::RocksDatabase;

pub fn hash(data: &[u8]) -> H256 {
    ids::hash(data).into()
}

/// Content-addressable storage database.
pub trait CASDatabase: Send {
    /// Clone ref to database instance.
    fn clone_boxed(&self) -> Box<dyn CASDatabase>;

    /// Read data by hash.
    fn read(&self, hash: &H256) -> Option<Vec<u8>>;

    /// Write data, returns data hash.
    fn write(&self, data: &[u8]) -> H256 {
        let hash = hash(data);
        self.write_by_hash(&hash, data);
        hash
    }

    /// Write data when hash is known.
    /// Note: should have debug check for hash match.
    fn write_by_hash(&self, hash: &H256, data: &[u8]);
}

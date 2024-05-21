// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

pub use ethereum_types;
pub use trie_db;
pub use hash_db;
pub use memory_db;
pub use rlp;

pub mod keccak_hasher;
pub mod rlp_node_codec;
pub mod patricia_trie;
pub mod types;

use keccak_hasher::KeccakHasher;

pub type MemoryDB = memory_db::MemoryDB::<KeccakHasher, memory_db::HashKey<KeccakHasher>, Vec<u8>>;

pub fn new_memory_db() -> MemoryDB {
    memory_db::MemoryDB::from_null_node(&rlp::NULL_RLP, rlp::NULL_RLP.as_ref().into())
}

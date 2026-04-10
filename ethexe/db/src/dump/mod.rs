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

//! Ethexe state dump for re-genesis.

mod apply;
mod collect;

use flate2::{Compression, read::DeflateDecoder, write::DeflateEncoder};
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::{BTreeMap, BTreeSet},
    io, path::Path,
};

/// State dump for ethexe re-genesis.
///
/// Contains a snapshot of all programs, their codes, and all
/// content-addressed storage blobs reachable from program states
/// at a given block.
#[derive(Debug, Clone, Encode, Decode)]
pub struct StateDump {
    /// Block hash for which this dump was created.
    /// This block becomes the new genesis.
    pub block_hash: H256,
    /// Valid code ids. Code bytes are stored in `blobs` (keyed by CodeId in CAS).
    pub codes: BTreeSet<CodeId>,
    /// Programs: program id -> (code id, state hash).
    pub programs: BTreeMap<ActorId, (CodeId, H256)>,
    /// All content-addressed storage blobs reachable from program state hashes,
    /// including original code blobs.
    /// Hashes are not stored explicitly — they are computable from the data.
    pub blobs: Vec<Vec<u8>>,
}

impl StateDump {
    /// Encode, compress with deflate, and write the dump to a file.
    pub fn write_to_file(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let encoded = self.encode();

        let file = std::fs::File::create(path)?;
        let mut encoder = DeflateEncoder::new(file, Compression::default());
        io::Write::write_all(&mut encoder, &encoded)?;
        encoder.finish()?;

        Ok(())
    }

    /// Read, decompress, and decode a dump from a file.
    pub fn read_from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let file = std::fs::File::open(path)?;
        let mut decoder = DeflateDecoder::new(file);
        let mut data = Vec::new();
        io::Read::read_to_end(&mut decoder, &mut data)?;

        Self::decode(&mut &data[..]).map_err(Into::into)
    }
}

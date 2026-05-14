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

mod collect;

use ethexe_common::{Announce, HashOf};
use flate2::{Compression, read::DeflateDecoder, write::DeflateEncoder};
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::Path,
};

/// State dump for ethexe re-genesis.
///
/// Contains a snapshot of all programs, their codes, and all
/// content-addressed storage blobs reachable from program states
/// at a given block.
#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub struct StateDump {
    /// Hash of the announce for which this dump was created.
    pub announce_hash: HashOf<Announce>,
    /// Block hash for which this dump was created.
    pub block_hash: H256,
    /// Valid code ids. Code bytes are stored in `blobs` (keyed by CodeId in CAS).
    pub codes: BTreeSet<CodeId>,
    /// Programs: program id -> (code id, state hash).
    pub programs: BTreeMap<ActorId, (CodeId, H256)>,
    /// All content-addressed storage blobs reachable from program state hashes,
    /// including original code blobs.
    /// Serialized as SCALE-encoded, deflate-compressed, hex-encoded single blob.
    #[serde(
        serialize_with = "serialize_blobs",
        deserialize_with = "deserialize_blobs"
    )]
    pub blobs: Vec<Vec<u8>>,
}

fn serialize_blobs<S: Serializer>(blobs: &Vec<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error> {
    let encoded = blobs.encode();

    let mut compressed = Vec::new();
    let mut encoder = DeflateEncoder::new(&mut compressed, Compression::default());
    io::Write::write_all(&mut encoder, &encoded).map_err(serde::ser::Error::custom)?;
    encoder.finish().map_err(serde::ser::Error::custom)?;

    let hex = format!("0x{}", hex::encode(&compressed));
    serializer.serialize_str(&hex)
}

fn deserialize_blobs<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Vec<u8>>, D::Error> {
    let hex_str: String = Deserialize::deserialize(deserializer)?;
    let hex_str = hex_str.strip_prefix("0x").unwrap_or(&hex_str);
    let compressed = hex::decode(hex_str).map_err(serde::de::Error::custom)?;

    let mut decoder = DeflateDecoder::new(&compressed[..]);
    let mut encoded = Vec::new();
    io::Read::read_to_end(&mut decoder, &mut encoded).map_err(serde::de::Error::custom)?;

    Vec::<Vec<u8>>::decode(&mut &encoded[..]).map_err(|e| serde::de::Error::custom(e.to_string()))
}

impl StateDump {
    /// Encode with SCALE, compress with deflate, and write the dump to a `.blob` file.
    pub fn write_to_blob(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let encoded = self.encode();

        let file = std::fs::File::create(path)?;
        let mut encoder = DeflateEncoder::new(file, Compression::default());
        io::Write::write_all(&mut encoder, &encoded)?;
        encoder.finish()?;

        Ok(())
    }

    /// Read and decode a dump from a `.blob` file.
    pub fn read_from_blob(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let file = std::fs::File::open(path)?;
        let mut decoder = DeflateDecoder::new(file);
        let mut data = Vec::new();
        io::Read::read_to_end(&mut decoder, &mut data)?;

        Self::decode(&mut &data[..]).map_err(Into::into)
    }

    /// Serialize as JSON and write to a `.json` file.
    pub fn write_to_json(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    /// Read and deserialize a dump from a `.json` file.
    pub fn read_from_json(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let file = std::fs::File::open(path)?;
        serde_json::from_reader(file).map_err(Into::into)
    }

    /// Read a dump file, auto-detecting format by extension (`.blob` or `.json`).
    pub fn read_from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        match path.as_ref().extension().and_then(|e| e.to_str()) {
            Some("blob") => Self::read_from_blob(path),
            Some("json") => Self::read_from_json(path),
            Some(ext) => anyhow::bail!("unsupported dump file extension: .{ext}"),
            None => anyhow::bail!("dump file must have .blob or .json extension"),
        }
    }

    /// Write a dump file, auto-detecting format by extension (`.blob` or `.json`).
    pub fn write_to_file(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        match path.as_ref().extension().and_then(|e| e.to_str()) {
            Some("blob") => self.write_to_blob(path),
            Some("json") => self.write_to_json(path),
            Some(ext) => anyhow::bail!("unsupported dump file extension: .{ext}"),
            None => anyhow::bail!("dump file must have .blob or .json extension"),
        }
    }
}

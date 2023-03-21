// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Metadata result

/// Metadata error
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Memory not exists")]
    MemoryNotExists,
    #[error("Metadata {0} not exists")]
    MetadataNotExists(String),
    #[error("Type {0} not found")]
    TypeNotFound(String),
    #[error("Type registry not found")]
    RegistryNotFound,
    #[error("Read {0} failed")]
    ReadMetadataFailed(String),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
    #[error(transparent)]
    Codec(#[from] parity_scale_codec::Error),
    #[error(transparent)]
    FromHex(#[from] hex::FromHexError),
}

/// Metadata result
pub type Result<T, E = Error> = std::result::Result<T, E>;

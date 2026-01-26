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

//! Core DKG protocol helpers.
//!
//! ```text
//! Round1: generate/receive commitments
//! Round2: generate/receive encrypted shares
//! Finalize: validate shares -> key material
//! ```

mod complaints;
mod finalize;
mod identifiers;
mod protocol;
mod round1;
mod round2;

pub use protocol::{DkgConfig, DkgProtocol, FinalizeResult};

#[cfg(test)]
mod tests;

type Ciphersuite = roast_secp256k1_evm::frost::Secp256K1Keccak256;
type Group = <Ciphersuite as roast_secp256k1_evm::frost::Ciphersuite>::Group;
type GroupSerialization = <Group as roast_secp256k1_evm::frost::Group>::Serialization;

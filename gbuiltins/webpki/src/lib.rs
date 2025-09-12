// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Type that should be used to create a message to the webpki built-in actor.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo)]
pub enum Request {
    /// Request Verify Certs Chain
    #[codec(index = 0)]
    VerifyCertsChain {
        ders: Vec<Vec<u8>>,
        sni: Vec<u8>,
        timestamp: u64,
    },
    /// Request Verify Signature
    #[codec(index = 1)]
    VerifySignature {
        der: Vec<u8>,
        message: Vec<u8>,
        signature: Vec<u8>,
        algo: u16,
    },
}

/// The enumeration contains result to a request.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo)]
pub enum Response {
    /// Result of Verify Certs Chain
    #[codec(index = 0)]
    VerifyCertsChain { certs_chain_ok: bool, dns_ok: bool },
    /// Result of the final exponentiation, encoded: [`ArkScale<Bls12_381::TargetField>`](https://docs.rs/ark-scale/).
    #[codec(index = 1)]
    VerifySignature { signature_ok: bool },
}

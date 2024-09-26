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

//! A trivial Plonky2 proof verification program:
//! - the `handle` function takes a payload in a form of concatenated binary encodings of
//!   `common_circuit_data` | `verifier_only_circuit_data` | `proof_with_public_inputs`,
//!   parses the payload and passes the deserialized structs to the Plonky2's `verify` function.
//!   Note that the payload is expected to be in the original Plonky2 binary format (not Scale).
//!   The output message would either contain "Success" (as a byte array) or an error message.

use super::{
    circuit::CustomPoseidonGoldilocksConfig as Config, serialize::parse_circuit_data_and_proof,
};
use gstd::{debug, msg, primitives::goldilocks_field::GoldilocksFieldWrapper as GF};

#[gstd::async_main]
async fn main() {
    let payload = msg::load_bytes().expect("Failed to load payload");

    let (verifier_circuit_data, proof) =
        match parse_circuit_data_and_proof::<GF, Config, 2>(&payload) {
            Ok(data) => data,
            Err(e) => {
                debug!("Failed to parse circuit data and proof: {}", e);
                msg::reply_bytes(b"Decoding error", 0).expect("Failed to send reply");
                return;
            }
        };

    match verifier_circuit_data.verify(proof) {
        Ok(_) => {
            msg::reply_bytes(b"Success", 0).expect("Failed to send reply");
        }
        Err(e) => {
            debug!("Failed to verify proof: {}", e);
            msg::reply_bytes(b"Verification error", 0).expect("Failed to send reply");
        }
    }
}

#[no_mangle]
extern "C" fn init() {}

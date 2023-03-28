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

//! Integration tests for command `key`
use crate::common::{self, traits::Convert, Result};

const SIGNATURE_PATT: &str = "Signature:";
const SEED_PATT: &str = "Seed:";
#[cfg(feature = "node-key")]
const SECRET_PATT: &str = "Secret:";
const PUBLIC_PATT: &str = "Public key:";

fn parse_from<'s>(log: &'s str, patt: &'s str) -> &'s str {
    let arr = log.split(patt).collect::<Vec<&str>>();
    arr[1].split_whitespace().collect::<Vec<&str>>()[0]
}

#[test]
fn test_sign_and_verify() -> Result<()> {
    // STDOUT of generate output template:
    //
    // Secret Phrase `cool link because slight minute face pelican among wise split timber museum` is account:
    //     Secret Seed:  0xb437604109791c3ea203cb840e29d7512e65251237bb927b65fae9bea0829c09
    //     Public key:   0xcafede900f53dae1b499d2a0d70898d631900e5918b2ba76bbcfafef9e5f007f
    //     SS58 Address: 5GesEyyr5EU1rGLt9JK72EYdvAeakqWNoKk5i7ZZSq9n6U3R
    let key_info = common::gcli(["key", "generate"])?.stdout.convert();
    let secret = parse_from(&key_info, SEED_PATT);
    let public = parse_from(&key_info, PUBLIC_PATT);

    // STDOUT of sign output template:
    //
    // Signature: 24d8d89e1a40ea6a1e076a598551062c21125877650085d6fde8f15c48ab3a65890eacfaeedddd22e23e3891f52610adac72fc6dbf0dcef5dbe133a96fd49087
    //     The signer of this signature is account:
    //     Secret Seed:  0xafaedcf860ebcda8c9439630d177d98cd3e799c1f7f1296a792e30263d3b120a
    //     Public key:   0xe4b6453570bc573eebe4143e40e023c7d702e2d8ffc0e3b39d4268671a3f1362
    //     SS58 Address: 5HEavLjXpVQtAWrsr7BjxXZysoW27zESHGpMX6nro7RGFEMA
    let message = "42";
    let sign_info = common::gcli(["key", "sign", secret, message])?
        .stdout
        .convert();
    let sig = parse_from(&sign_info, SIGNATURE_PATT);

    // STDOUT of verify output template:
    //
    // Result: true
    let verify_info = common::gcli(["key", "verify", sig, message, public])?
        .stdout
        .convert();

    assert!(verify_info.contains("true"));
    Ok(())
}

#[test]
#[cfg(feature = "node-key")]
fn test_node_key() -> Result<()> {
    // template STDOUT
    //
    // Secret:  0x510b7a90ac2050b8952682489da36f5064f0b7348f3da557dacc36ae8c66cc99
    // Peer ID: 12D3KooWQEUQzpFif7Kv7BgGpniQPat8X1tjLkogrNyL4cww51MR
    let key_info = common::gcli(["key", "generate-node-key"])?.stdout.convert();
    let secret = parse_from(&key_info, SECRET_PATT);

    // template STDOUT
    //
    // Peer ID: 12D3KooWQEUQzpFif7Kv7BgGpniQPat8X1tjLkogrNyL4cww51MR
    let inspect_info = common::gcli(["key", "inspect-node-key", secret])?
        .stdout
        .convert();

    assert!(key_info.contains(&inspect_info));
    Ok(())
}

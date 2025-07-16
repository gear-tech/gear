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

use alloy::primitives::{Address, FixedBytes, hex};

const INITIALIZATION_CODE: FixedBytes<10> = FixedBytes::new(hex!("3d602d80600a3d3981f3"));
const RUNTIME_CODE_1: FixedBytes<10> = FixedBytes::new(hex!("363d3d373d3d3d363d73"));
const RUNTIME_CODE_2: FixedBytes<15> = FixedBytes::new(hex!("5af43d82803e903d91602b57fd5bf3"));

pub const fn minimal_proxy_bytecode(address: [u8; 20]) -> [u8; 55] {
    let address = Address::new(address);
    let part1: FixedBytes<20> = INITIALIZATION_CODE.concat_const(RUNTIME_CODE_1);
    let part2: FixedBytes<40> = part1.concat_const(address.0);
    let part3: FixedBytes<55> = part2.concat_const(RUNTIME_CODE_2);
    part3.0
}

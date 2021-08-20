// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use gstd_meta::*;

#[allow(dead_code)]
#[derive(TypeInfo)]
struct Carrot {
    fresh: bool,
    size: u8,
}

#[allow(dead_code)]
#[derive(TypeInfo)]
struct Bread {
    roasted: bool,
    width: u8,
}

#[allow(dead_code)]
#[derive(TypeInfo)]
struct Sandwich {
    bread: Bread,
    price: Option<u64>,
}

#[allow(dead_code)]
#[derive(TypeInfo)]
struct Salad {
    vegetables: Vec<Carrot>,
    finished: Result<u64, u8>,
}

meta! {
    title: "Example program with metadata",
    input: Bread,
    output: Sandwich,
    init_input: Salad,
    init_output: Salad,
    extra_types: Carrot
}

#[test]
fn title() {
    let _: *mut [i32; 2] = unsafe { meta_title() };
}

#[test]
fn input() {
    let _: *mut [i32; 2] = unsafe { meta_input() };
}

#[test]
fn output() {
    let _: *mut [i32; 2] = unsafe { meta_output() };
}

#[test]
fn init_input() {
    let _: *mut [i32; 2] = unsafe { meta_init_input() };
}

#[test]
fn init_output() {
    let _: *mut [i32; 2] = unsafe { meta_init_output() };
}

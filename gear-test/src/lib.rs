// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Crate defines functionality to test wasm programs.
//!
//! You have several ways to test smart contracts. This crate provides developers with an opportunity
//! to define test strategies in some *test.yaml* file. Crate's test runner will run all the tests and return found errors in case they
//! occurred. One of the main usages of the current crate is done by Gear [node](../gear_node/index.html): when running node executable you can define run
//! options and one of them is to run tests. One of the main features of such run is that node's key-value storage will be used.

pub mod address;
pub mod check;
pub mod js;
pub mod manager;
pub mod proc;
pub mod sample;

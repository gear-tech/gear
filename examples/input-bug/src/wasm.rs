// This file is part of Gear.

// Copyright (C) 2023-2024 Gear Technologies Inc.
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

use gstd::{
    msg::{self, MessageHandle},
    prelude::*,
};

#[no_mangle]
extern "C" fn handle() {
    let mh = MessageHandle::init().expect("failed MessageHandle::init");
    mh.push_input(50..100).expect("failed push input");
    let mid = mh.commit(msg::source(), 0).expect("failed sending message");
    dbg!(mid);
}

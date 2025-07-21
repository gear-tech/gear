// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::{Call, Scheme};
use gstd::{String, Vec, collections::BTreeMap, msg, prelude::*};

pub(crate) static mut DATA: BTreeMap<String, Vec<u8>> = BTreeMap::new();
static mut SCHEME: Option<Scheme> = None;

fn process_fn<'a>(f: impl Fn(&'a Scheme) -> Option<&'a Vec<Call>>) {
    let scheme = unsafe { static_ref!(SCHEME).as_ref() }.expect("Should be set before access");
    let calls = f(scheme)
        .cloned()
        .unwrap_or_else(|| msg::load().expect("Failed to load payload"));

    let mut res = None;

    for call in calls {
        res = Some(call.process(res));
    }
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    let scheme = msg::load().expect("Failed to load payload");
    unsafe { SCHEME = Some(scheme) };

    process_fn(|scheme| Some(scheme.init()));
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    process_fn(Scheme::handle);
}

#[unsafe(no_mangle)]
extern "C" fn handle_reply() {
    process_fn(Scheme::handle_reply);
}

#[unsafe(no_mangle)]
extern "C" fn handle_signal() {
    process_fn(Scheme::handle_signal);
}

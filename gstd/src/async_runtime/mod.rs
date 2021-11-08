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

mod futures;
mod signals;

pub use self::futures::event_loop;

use signals::WakeSignals;
use self::futures::FuturesMap;
pub(crate) use signals::ReplyPoll;
use crate::prelude::BTreeMap;

static mut FUTURES: Option<FuturesMap> = None;

pub(crate) fn futures() -> &'static mut FuturesMap {
    unsafe { FUTURES.get_or_insert_with(BTreeMap::new) }
}

static mut SIGNALS: Option<WakeSignals> = None;

pub(crate) fn signals() -> &'static mut WakeSignals {
    unsafe {
        SIGNALS.get_or_insert_with(WakeSignals::new)
    }
}

#[allow(clippy::missing_safety_doc)]
#[no_mangle]
pub unsafe extern "C" fn handle_reply() {
    signals().record_reply();
}

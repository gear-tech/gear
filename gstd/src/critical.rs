// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

//! Critical section that guarantees code section execution
//!
//! Code is executed in case of failure in `handle_signal` entry point
//! only across [`wait`](crate::exec::wait) or `.await` calls.
//!
//! ```rust,ignore
//! use gstd::msg;
//! use gstd::critical;
//!
//! // get source outside of critical section
//! // because `gr_source` sys-call is forbidden inside `handle_signal` entry point
//! let source = msg::source();
//! let section = critical::Section::new(move || {
//!     msg::send(source, "example", 0).expect("Failed to send message");
//! });
//!
//! msg::send_bytes_for_reply(msg::source(), b"for_reply", 0, 0)
//!     .expect("Failed to send message")
//!     .await
//!     .expect("Received error reply");
//!
//! section.execute();
//!
//! ```

use alloc::{boxed::Box, collections::BTreeMap};
use core::{any::TypeId, mem};

static mut SECTIONS: Sections = Sections::new();

pub(crate) struct Sections {
    fns: BTreeMap<TypeId, Box<dyn FnMut()>>,
}

impl Sections {
    const fn new() -> Self {
        Self {
            fns: BTreeMap::new(),
        }
    }

    pub(crate) fn get() -> &'static mut Self {
        unsafe { &mut SECTIONS }
    }

    fn register<F>(&mut self, f: F) -> Section<F>
    where
        F: FnMut() + Clone + 'static,
    {
        self.fns.insert(TypeId::of::<F>(), Box::new(f.clone()));
        Section(f)
    }

    fn unregister<F>(&mut self)
    where
        F: FnMut() + Clone + 'static,
    {
        self.fns.remove(&TypeId::of::<F>());
    }

    /// Executes every saved critical section once.
    ///
    /// Must be called in `handle_signal` entry point if you don't use async runtime.
    pub fn execute_all(&mut self) {
        for (_, mut f) in mem::take(&mut self.fns) {
            (f)();
        }
    }
}

/// Critical section.
pub struct Section<F>(F)
where
    F: FnMut() + Clone + 'static;

impl<F> Section<F>
where
    F: FnMut() + Clone + 'static,
{
    /// Creates a new critical section.
    pub fn new(f: F) -> Self {
        Sections::get().register(f)
    }

    /// Executes critical section.
    pub fn execute(mut self) {
        (self.0)()
    }
}

impl<F> Drop for Section<F>
where
    F: FnMut() + Clone + 'static,
{
    fn drop(&mut self) {
        Sections::get().unregister::<F>();
    }
}

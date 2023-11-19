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
//! Code is executed in `handle_signal` entry point in case of failure
//! only across [`wait`](crate::exec::wait) or `.await` calls
//! because sections have to be saved.
//!
//! ```rust,no_run
//! use gstd::{critical, msg};
//!
//! # async fn _dummy() {
//!
//! // get source outside of critical section
//! // because `gr_source` sys-call is forbidden inside `handle_signal` entry point
//! let source = msg::source();
//! // register section
//! let section = critical::Section::new(move || {
//!     msg::send(source, "example", 0).expect("Failed to send message");
//! });
//!
//! // section is now saved
//! msg::send_for_reply(msg::source(), "for_reply", 0, 0)
//!     .expect("Failed to send message")
//!     .await
//!     .expect("Received error reply");
//!
//! // if some code fails (panic, out of gas, etc) after `wait` (`send_for_reply` in our case)
//! // then saved sections will be executed in `handle_signal`
//!
//! // your code
//! // ...
//!
//! // execute section
//! section.execute();
//!
//! # }
//! ```

use alloc::boxed::Box;
use core::{any::TypeId, mem};
use hashbrown::HashMap;

static mut SECTIONS: Option<Sections> = None;

pub(crate) struct Sections {
    fns: HashMap<TypeId, Box<dyn FnMut()>>,
}

impl Sections {
    fn new() -> Self {
        Self {
            fns: HashMap::new(),
        }
    }

    pub(crate) fn get() -> &'static mut Self {
        unsafe { SECTIONS.get_or_insert_with(Self::new) }
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
    /// Must be called in `handle_signal` entry point
    /// if you don't use async runtime.
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

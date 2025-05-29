// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use std::{
    cell::RefCell,
    future,
    task::{Context, Poll},
    thread,
};

/// Exactly like [`tokio::task_local`] but provides mutable access and returns value.
#[macro_export]
macro_rules! task_local {
    ($(#[$attr:meta])* static $vis:vis $name:ident: $t:ty;) => {
        $(#[$attr])*
        $vis static $name: $crate::task_local::LocalKey<$t> = {
            std::thread_local! {
                static __KEY: std::cell::RefCell<Option<$t>> = const { std::cell::RefCell::new(None) };
            }

            $crate::task_local::LocalKey { inner: __KEY }
        };
    };
}

pub struct LocalKey<T: 'static> {
    #[doc(hidden)]
    pub inner: thread::LocalKey<RefCell<Option<T>>>,
}

impl<T: 'static> LocalKey<T> {
    pub fn scope<F, R>(&'static self, value: T, f: F) -> (T, R)
    where
        F: FnOnce() -> R,
    {
        self.inner.set(Some(value));
        let res = f();
        let value = self.inner.take().expect("value is set above");
        (value, res)
    }

    pub fn with_mut<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        self.inner.with_borrow_mut(|value| {
            let value = value.as_mut().expect("value is not set");
            f(value)
        })
    }

    pub fn poll_fn<F, R>(&'static self, mut f: F) -> impl Future<Output = R>
    where
        F: FnMut(&mut Context<'_>, &mut T) -> Poll<R>,
    {
        future::poll_fn(move |cx| self.with_mut(|value| f(cx, value)))
    }
}

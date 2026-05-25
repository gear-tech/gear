// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
        $vis static $name: $crate::LocalKey<$t> = {
            ::std::thread_local! {
                static __KEY: ::std::cell::RefCell<Option<$t>> = const { ::std::cell::RefCell::new(None) };
            }

            $crate::LocalKey { inner: __KEY }
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

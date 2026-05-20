// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use futures::{FutureExt, ready};
use std::{
    fmt::Debug,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::time::{self, Sleep};

/// Asynchronous timer with inner data kept.
#[derive(Debug)]
pub struct Timer<T = ()> {
    /// Name of the timer.
    name: &'static str,

    /// Duration of the timer.
    duration: Duration,

    /// Moment of time when the timer was started and applied data.
    inner: Option<(Pin<Box<Sleep>>, T)>,
}

impl<T: Debug> Timer<T> {
    /// Create a new timer with a name and a duration.
    pub fn new(name: &'static str, duration: Duration) -> Self {
        log::trace!("New timer '{name}' with duration {duration:?} created");

        Self {
            name,
            duration,
            inner: None,
        }
    }

    /// Create a new timer with a name and a duration in seconds.
    pub fn new_from_secs(name: &'static str, sec: u64) -> Self {
        Self::new(name, Duration::from_secs(sec))
    }

    /// Create a new timer with a name and a duration in milliseconds.
    pub fn new_from_millis(name: &'static str, millis: u64) -> Self {
        Self::new(name, Duration::from_millis(millis))
    }

    /// Check if the timer has started.
    pub fn started(&self) -> bool {
        self.inner.is_some()
    }

    /// Start the timer from the beginning with new data.
    /// Returns the previous data if the timer was already started.
    pub fn start(&mut self, data: T) -> Option<T> {
        log::trace!("Started timer '{}' with data {data:?}", self.name);

        self.inner
            .replace((Box::pin(time::sleep(self.duration)), data))
            .map(|(_, data)| data)
    }

    /// Stop the timer and return the data, if it was started.
    pub fn stop(&mut self) -> Option<T> {
        log::trace!("Stopped timer '{}'", self.name);

        self.inner.take().map(|(_, data)| data)
    }
}

impl<T: Clone> Clone for Timer<T> {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            duration: self.duration,
            inner: self
                .inner
                .as_ref()
                .map(|(sleep, data)| (Box::pin(time::sleep_until(sleep.deadline())), data.clone())),
        }
    }
}

impl<T: Debug + Unpin> Future for Timer<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some((sleep, _)) = self.inner.as_mut() {
            ready!(sleep.poll_unpin(cx));

            let data = self.inner.take().map(|(_, data)| data).unwrap();

            log::debug!("Timer '{}' with data {:?} rings", self.name, data);

            return Poll::Ready(data);
        }

        Poll::Pending
    }
}

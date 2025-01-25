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

use crate::OptionFuture;
use std::{
    fmt::Debug,
    time::{Duration, Instant},
};
use tokio::time;

/// Asynchronous timer with inner data kept.
pub struct Timer<T = ()>
where
    T: Debug,
{
    /// Name of the timer.
    name: &'static str,

    /// Duration of the timer.
    duration: Duration,

    /// Moment of time when the timer was started and applied data.
    inner: Option<(Instant, T)>,
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

    /// Get the remaining time until the timer will be ready to ring if started.
    pub fn remaining(&self) -> Option<Duration> {
        self.inner.as_ref().map(|(start, _)| {
            self.duration
                .checked_sub(start.elapsed())
                .unwrap_or(Duration::ZERO)
        })
    }

    /// Start the timer from the beginning with new data.
    /// Returns the previous data if the timer was already started.
    pub fn start(&mut self, data: T) -> Option<T> {
        log::trace!("Started timer '{}' with data {data:?}", self.name);

        self.inner
            .replace((Instant::now(), data))
            .map(|(_, data)| data)
    }

    /// Stop the timer and return the data, if it was started.
    pub fn stop(&mut self) -> Option<T> {
        log::trace!("Stopped timer '{}'", self.name);

        self.inner.take().map(|(_, data)| data)
    }

    /// Result of time passed - timer's ring.
    pub async fn rings(&mut self) -> T {
        self.remaining()
            .map(async |dur| {
                if !dur.is_zero() {
                    log::trace!("Timer {} will ring in {dur:?}", self.name);
                }

                time::sleep(dur).await;

                log::trace!("Timer {} rings!", self.name);

                self.inner
                    .take()
                    .map(|(_, data)| data)
                    .expect("stopped or not started timer cannot ring;")
            })
            .maybe()
            .await
    }
}

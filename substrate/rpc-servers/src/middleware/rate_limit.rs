// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! RPC rate limit.

use governor::{
    clock::{DefaultClock, QuantaClock},
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota,
};
use std::{num::NonZeroU32, sync::Arc};

type RateLimitInner = governor::RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

/// Rate limit.
#[derive(Debug, Clone)]
pub struct RateLimit {
    pub(crate) inner: Arc<RateLimitInner>,
    pub(crate) clock: QuantaClock,
}

impl RateLimit {
    /// Create a new `RateLimit` per minute.
    pub fn per_minute(n: NonZeroU32) -> Self {
        let clock = QuantaClock::default();
        Self {
            inner: Arc::new(RateLimitInner::direct_with_clock(
                Quota::per_minute(n),
                &clock,
            )),
            clock,
        }
    }
}

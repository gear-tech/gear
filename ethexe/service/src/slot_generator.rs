// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

//! Slot generator.
//!
//! Emits [`SlotEvent::SlotStarted`] each time a new Ethereum slot begins.
//!
//! Three implementations of the [`SlotGenerator`] trait are provided,
//! matched to different deployment scenarios:
//!
//! - [`SystemTimeSlotGenerator`] — wall-clock ticks aligned to slot
//!   boundaries derived from [`ProtocolTimelines`]. Used in production
//!   and in tests that rely on continuous Ethereum block generation.
//! - [`PerBlockSlotGenerator`] — ticks driven solely by the timestamp
//!   of each arriving Ethereum block. Fully deterministic; used in
//!   tests that want to control slot progression manually.
//! - [`HybridSlotGenerator`] — ticks driven by block arrival *and* an
//!   internal `slot_duration` timer that fills in the gaps when no
//!   blocks arrive. Used in tests with on-demand Anvil block mining.
//!
//! All implementations guarantee strict monotonicity: a slot index is
//! emitted at most once, and only if it is strictly greater than every
//! previously emitted value.
//!
//! In [`PerBlockSlotGenerator`] and [`HybridSlotGenerator`] the slot
//! index is a pure counter — `last_emitted + 1` on timer-driven ticks,
//! `slot_from_ts(block_ts)` on block-driven ticks. Wall-clock time is
//! not consulted. This keeps tests deterministic under `tokio::time::pause`.

use ethexe_common::ProtocolTimelines;
use futures::{FutureExt, Stream, stream::FusedStream};
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::{Instant, Interval, MissedTickBehavior, Sleep, interval_at, sleep};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotEvent {
    /// A new slot has started. The payload is the absolute slot index.
    SlotStarted(u64),
}

/// Abstract source of [`SlotEvent`]s.
pub trait SlotGenerator: Stream<Item = SlotEvent> + FusedStream + Send + Unpin {
    /// Notify the generator about a new Ethereum block timestamp.
    fn on_new_block(&mut self, block_ts: u64);

    /// Manually trigger emission of the next slot.
    ///
    /// Default: no-op. Only honored by [`PerBlockSlotGenerator`] where
    /// tests may need to advance the slot without delivering a new
    /// Ethereum block.
    fn trigger_next_slot(&mut self) {}
}

// -------------------------------------------------------------------------
// SystemTimeSlotGenerator — production.
// -------------------------------------------------------------------------

/// Wall-clock slot generator aligned to slot boundaries.
///
/// Anchored once at construction to a tokio [`Instant`] that corresponds
/// to `genesis_ts`. All subsequent slot numbers are derived as
/// `(Instant::now() - genesis_instant) / slot`, so the calculation stays
/// consistent under `tokio::time::pause`/`advance` in tests.
pub struct SystemTimeSlotGenerator {
    timelines: ProtocolTimelines,
    last_emitted: Option<u64>,
    interval: Interval,
    /// Tokio [`Instant`] at which slot 0 begins.
    genesis_instant: Instant,
}

impl SystemTimeSlotGenerator {
    pub fn new(timelines: ProtocolTimelines) -> Self {
        assert_non_zero_slot(&timelines);
        let now_unix_ts = now_unix_secs();
        let now_instant = Instant::now();
        let duration_from_genesis_in_secs = now_unix_ts
            .checked_sub(timelines.genesis_ts)
            .expect("system time is before genesis_ts");
        let genesis_instant = now_instant - Duration::from_secs(duration_from_genesis_in_secs);

        // First tick at the boundary of the next not started slot.
        let next_slot = timelines.slot_from_ts(now_unix_ts) + 1;
        let first_tick = genesis_instant + Duration::from_secs(next_slot * timelines.slot);
        let mut interval = interval_at(first_tick, Duration::from_secs(timelines.slot));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        Self {
            timelines,
            last_emitted: None,
            interval,
            genesis_instant,
        }
    }
}

impl Stream for SystemTimeSlotGenerator {
    type Item = SlotEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            let _ = std::task::ready!(self.interval.poll_tick(cx));

            let elapsed = Instant::now().saturating_duration_since(self.genesis_instant);
            let slot = elapsed.as_secs() / self.timelines.slot;

            if matches!(self.last_emitted, Some(prev) if slot <= prev) {
                continue;
            }

            self.last_emitted = Some(slot);
            return Poll::Ready(Some(SlotEvent::SlotStarted(slot)));
        }
    }
}

impl FusedStream for SystemTimeSlotGenerator {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl SlotGenerator for SystemTimeSlotGenerator {
    fn on_new_block(&mut self, _block_ts: u64) {
        // Wall-clock driven — block arrivals are irrelevant.
    }
}

// -------------------------------------------------------------------------
// PerBlockSlotGenerator — fully deterministic, blocks only.
// -------------------------------------------------------------------------

/// Slot generator driven exclusively by Ethereum block timestamps.
///
/// The only way to advance the frontier is via [`Self::on_new_block`]
/// or the escape hatch [`SlotGenerator::trigger_next_slot`].
pub struct PerBlockSlotGenerator {
    timelines: ProtocolTimelines,
    last_emitted: Option<u64>,
    pending: VecDeque<u64>,
}

impl PerBlockSlotGenerator {
    pub fn new(timelines: ProtocolTimelines) -> Self {
        assert_non_zero_slot(&timelines);
        Self {
            timelines,
            last_emitted: None,
            pending: VecDeque::new(),
        }
    }

    fn enqueue(&mut self, slot: u64) {
        let frontier = self.pending.back().copied().or(self.last_emitted);
        if matches!(frontier, Some(f) if slot <= f) {
            return;
        }
        self.pending.push_back(slot);
    }
}

impl Stream for PerBlockSlotGenerator {
    type Item = SlotEvent;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(slot) = self.pending.pop_front() {
            self.last_emitted = Some(slot);
            return Poll::Ready(Some(SlotEvent::SlotStarted(slot)));
        }
        Poll::Pending
    }
}

impl FusedStream for PerBlockSlotGenerator {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl SlotGenerator for PerBlockSlotGenerator {
    fn on_new_block(&mut self, block_ts: u64) {
        let slot = self.timelines.slot_from_ts(block_ts);

        self.enqueue(slot);
    }

    fn trigger_next_slot(&mut self) {
        let next = self.last_emitted.map(|s| s + 1).unwrap_or(0);
        self.enqueue(next);
    }
}

// -------------------------------------------------------------------------
// HybridSlotGenerator — blocks + internal timer fallback.
// -------------------------------------------------------------------------

/// Slot generator driven by block arrivals with a one-shot `slot_duration`
/// timer for the slot immediately after each block.
///
/// Protocol: a new block emits `block.slot` and arms a one-shot timer; when
/// that timer fires it emits `last_emitted + 1` and disarms. After the
/// timer has fired, no further slots are emitted until the next block
/// arrives.
///
/// If a block arrives with `block.slot > last_emitted + 1`, the skipped
/// intermediate slots are not re-emitted — the generator jumps directly
/// to `block.slot` and starts a fresh one-shot timer from there.
pub struct HybridSlotGenerator {
    timelines: ProtocolTimelines,
    last_emitted: Option<u64>,
    pending: Option<u64>,
    timer: Option<Pin<Box<Sleep>>>,
}

impl HybridSlotGenerator {
    pub fn new(timelines: ProtocolTimelines) -> Self {
        assert_non_zero_slot(&timelines);
        Self {
            timelines,
            last_emitted: None,
            pending: None,
            timer: None,
        }
    }
}

impl Stream for HybridSlotGenerator {
    type Item = SlotEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(slot) = self.pending.take() {
            self.last_emitted = Some(slot);
            return Poll::Ready(Some(SlotEvent::SlotStarted(slot)));
        }

        if let Some(timer) = self.timer.as_mut()
            && timer.poll_unpin(cx).is_ready()
        {
            // One-shot: drop the timer and schedule `last_emitted + 1`.
            // The next slot will only be emitted when a new block arrives.
            self.timer = None;
            if let Some(last) = self.last_emitted {
                self.last_emitted = Some(last + 1);
                return Poll::Ready(Some(SlotEvent::SlotStarted(last + 1)));
            }
        }

        return Poll::Pending;
    }
}

impl FusedStream for HybridSlotGenerator {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl SlotGenerator for HybridSlotGenerator {
    fn on_new_block(&mut self, block_ts: u64) {
        let slot = self.timelines.slot_from_ts(block_ts);

        let frontier = self.pending.or(self.last_emitted);
        if matches!(frontier, Some(f) if slot <= f) {
            return;
        }
        self.pending = Some(slot);
        // Block arrival arms a one-shot timer that will fill the next slot.
        self.timer = Some(Box::pin(sleep(Duration::from_secs(self.timelines.slot))));
    }
}

// -------------------------------------------------------------------------
// Helpers.
// -------------------------------------------------------------------------

// TODO: drop once PR #5293 lands on master and `ProtocolTimelines::slot`
// becomes `NonZero<u64>` at the type level.
fn assert_non_zero_slot(timelines: &ProtocolTimelines) {
    assert!(
        timelines.slot > 0,
        "ProtocolTimelines.slot must be non-zero"
    );
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    fn timelines(genesis_ts: u64, slot: u64) -> ProtocolTimelines {
        ProtocolTimelines {
            genesis_ts,
            era: slot * 1000,
            election: slot * 100,
            slot,
        }
    }

    // ---- PerBlockSlotGenerator ----

    #[tokio::test]
    async fn per_block_emits_on_monotonic_slot_crossing() {
        let mut sg = PerBlockSlotGenerator::new(timelines(1000, 10));

        sg.on_new_block(1005); // slot 0
        sg.on_new_block(1015); // slot 1
        sg.on_new_block(1030); // slot 3

        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(0)));
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(1)));
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(3)));
    }

    #[tokio::test]
    async fn per_block_skips_duplicate_and_older_slots() {
        let mut sg = PerBlockSlotGenerator::new(timelines(1000, 10));

        sg.on_new_block(1025); // slot 2
        sg.on_new_block(1025); // same slot — skip
        sg.on_new_block(1015); // older slot — skip
        sg.on_new_block(1035); // slot 3 — emit

        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(2)));
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(3)));

        let poll = futures::poll!(sg.next());
        assert!(poll.is_pending());
    }

    #[tokio::test]
    async fn per_block_trigger_advances_without_block() {
        let mut sg = PerBlockSlotGenerator::new(timelines(1000, 10));

        sg.on_new_block(1025); // slot 2
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(2)));

        sg.trigger_next_slot();
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(3)));

        sg.trigger_next_slot();
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(4)));
    }

    // ---- HybridSlotGenerator ----

    #[tokio::test(start_paused = true)]
    async fn hybrid_first_emission_requires_block() {
        let mut sg = HybridSlotGenerator::new(timelines(1000, 10));

        // No block yet — the timer does not start on its own.
        tokio::time::advance(Duration::from_secs(100)).await;
        let poll = futures::poll!(sg.next());
        assert!(poll.is_pending());

        sg.on_new_block(1025); // slot 2
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(2)));
    }

    #[tokio::test(start_paused = true)]
    async fn hybrid_timer_fires_once_per_block() {
        let slot_secs = 10;
        let mut sg = HybridSlotGenerator::new(timelines(1000, slot_secs));

        sg.on_new_block(1025); // slot 2
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(2)));

        // One slot_duration elapses — timer fires and emits `last_emitted + 1`.
        tokio::time::advance(Duration::from_secs(slot_secs)).await;
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(3)));

        // Further time advances produce no more emissions — the timer is
        // one-shot; the next slot only comes from a block arrival.
        tokio::time::advance(Duration::from_secs(slot_secs * 5)).await;
        let poll = futures::poll!(sg.next());
        assert!(poll.is_pending());

        // A new block resumes emissions.
        sg.on_new_block(1075); // slot 7
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(7)));
    }

    #[tokio::test(start_paused = true)]
    async fn hybrid_block_resets_timer_and_jumps_ahead() {
        let slot_secs = 10;
        let mut sg = HybridSlotGenerator::new(timelines(1000, slot_secs));

        sg.on_new_block(1025); // slot 2
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(2)));

        // Block jumps ahead by several slots — skipped slots are *not* emitted.
        sg.on_new_block(1075); // slot 7
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(7)));

        // Timer was reset to the moment slot 7 was emitted — the next tick
        // is slot_duration later and yields slot 8, not earlier.
        let before = futures::poll!(sg.next());
        assert!(before.is_pending());

        tokio::time::advance(Duration::from_secs(slot_secs)).await;
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(8)));
    }

    #[tokio::test(start_paused = true)]
    async fn hybrid_block_matching_next_timer_slot_is_deduped() {
        let slot_secs = 10;
        let mut sg = HybridSlotGenerator::new(timelines(1000, slot_secs));

        sg.on_new_block(1025); // slot 2
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(2)));

        // Timer fires first.
        tokio::time::advance(Duration::from_secs(slot_secs)).await;
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(3)));

        // A block with the same slot 3 arrives later — it is skipped.
        sg.on_new_block(1035); // slot 3 — duplicate
        let poll = futures::poll!(sg.next());
        assert!(poll.is_pending());
    }

    // ---- SystemTimeSlotGenerator ----

    #[tokio::test(start_paused = true)]
    async fn system_time_emits_one_slot_per_period() {
        let slot_secs = 4;
        let mut sg = SystemTimeSlotGenerator::new(timelines(0, slot_secs));

        let first_wait = {
            let now = now_unix_secs();
            let next = (now / slot_secs + 1) * slot_secs;
            Duration::from_secs(next - now)
        };
        tokio::time::advance(first_wait).await;
        let SlotEvent::SlotStarted(first) = sg.next().await.unwrap();

        tokio::time::advance(Duration::from_secs(slot_secs)).await;
        let SlotEvent::SlotStarted(second) = sg.next().await.unwrap();

        assert_eq!(second, first + 1);
    }

    #[tokio::test(start_paused = true)]
    async fn system_time_skips_missed_ticks_without_drift() {
        let slot_secs = 4;
        let mut sg = SystemTimeSlotGenerator::new(timelines(0, slot_secs));

        let first_wait = {
            let now = now_unix_secs();
            let next = (now / slot_secs + 1) * slot_secs;
            Duration::from_secs(next - now)
        };
        tokio::time::advance(first_wait).await;
        let SlotEvent::SlotStarted(first) = sg.next().await.unwrap();

        tokio::time::advance(Duration::from_secs(slot_secs * 3)).await;
        let SlotEvent::SlotStarted(next) = sg.next().await.unwrap();
        assert_eq!(next, first + 3);

        tokio::time::advance(Duration::from_secs(slot_secs)).await;
        let SlotEvent::SlotStarted(after) = sg.next().await.unwrap();
        assert_eq!(after, next + 1);
    }
}

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
/// The current slot at each tick is derived from
/// `base_unix_secs + tokio_elapsed_since_base`, so the derivation stays
/// consistent under `tokio::time::pause`/`advance` in tests.
pub struct SystemTimeSlotGenerator {
    timelines: ProtocolTimelines,
    last_emitted: Option<u64>,
    interval: Interval,
    base_instant: Instant,
    base_unix_secs: u64,
}

impl SystemTimeSlotGenerator {
    pub fn new(timelines: ProtocolTimelines) -> Self {
        assert_non_zero_slot(&timelines);
        let base_unix_secs = now_unix_secs();
        let base_instant = Instant::now();
        let interval = build_aligned_interval(&timelines, base_unix_secs, base_instant);
        Self {
            timelines,
            last_emitted: None,
            interval,
            base_instant,
            base_unix_secs,
        }
    }
}

impl Stream for SystemTimeSlotGenerator {
    type Item = SlotEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            let _ = std::task::ready!(self.interval.poll_tick(cx));

            let elapsed = Instant::now().saturating_duration_since(self.base_instant);
            let now_ts = self.base_unix_secs.saturating_add(elapsed.as_secs());
            let Some(slot) = slot_from_ts(&self.timelines, now_ts) else {
                continue;
            };

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
        let Some(slot) = slot_from_ts(&self.timelines, block_ts) else {
            return;
        };
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

/// Slot generator that emits on whichever comes first:
/// a new block that advances the frontier, or a `slot_duration` timer
/// started when the last slot was emitted.
///
/// If a block arrives with `block.slot > last_emitted + 1`, the skipped
/// intermediate slots are not re-emitted — the generator jumps directly
/// to `block.slot` and the timer is restarted from there.
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

    fn slot_duration(&self) -> Duration {
        Duration::from_secs(self.timelines.slot)
    }

    fn restart_timer(&mut self) {
        self.timer = Some(Box::pin(sleep(self.slot_duration())));
    }

    fn try_advance_to(&mut self, slot: u64) -> bool {
        let frontier = self.pending.or(self.last_emitted);
        if matches!(frontier, Some(f) if slot <= f) {
            return false;
        }
        self.pending = Some(slot);
        true
    }
}

impl Stream for HybridSlotGenerator {
    type Item = SlotEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if let Some(slot) = self.pending.take() {
                self.last_emitted = Some(slot);
                // Ensure there is an active timer so the gap after this
                // emission is bounded by slot_duration.
                self.restart_timer();
                return Poll::Ready(Some(SlotEvent::SlotStarted(slot)));
            }

            if let Some(timer) = self.timer.as_mut()
                && timer.poll_unpin(cx).is_ready()
            {
                // Timer fired — schedule `last_emitted + 1` and loop so the
                // pending branch above delivers it and restarts the timer.
                if let Some(last) = self.last_emitted {
                    self.pending = Some(last + 1);
                    continue;
                }
                // Defensive: no prior emission means nothing to increment.
                // Drop the timer to avoid spinning; a block must arrive first.
                self.timer = None;
            }

            return Poll::Pending;
        }
    }
}

impl FusedStream for HybridSlotGenerator {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl SlotGenerator for HybridSlotGenerator {
    fn on_new_block(&mut self, block_ts: u64) {
        let Some(slot) = slot_from_ts(&self.timelines, block_ts) else {
            return;
        };
        if !self.try_advance_to(slot) {
            return;
        }
        // Any block that advances the frontier resets the fallback timer.
        self.restart_timer();
    }
}

// -------------------------------------------------------------------------
// Helpers.
// -------------------------------------------------------------------------

fn build_aligned_interval(
    timelines: &ProtocolTimelines,
    now_secs: u64,
    now_instant: Instant,
) -> Interval {
    let slot_secs = timelines.slot;
    let period = Duration::from_secs(slot_secs);

    // Wait until the start of the next slot boundary, so the first tick
    // falls on a real slot transition rather than at an arbitrary offset.
    let wait_secs = if now_secs < timelines.genesis_ts {
        // Pre-genesis: wait until genesis, then the regular period takes over.
        timelines.genesis_ts - now_secs
    } else {
        let next_slot = (now_secs - timelines.genesis_ts) / slot_secs + 1;
        let next_boundary = timelines.genesis_ts + next_slot * slot_secs;
        next_boundary - now_secs
    };

    let start = now_instant + Duration::from_secs(wait_secs);
    let mut interval = interval_at(start, period);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    interval
}

fn slot_from_ts(timelines: &ProtocolTimelines, ts_secs: u64) -> Option<u64> {
    ts_secs
        .checked_sub(timelines.genesis_ts)
        .map(|delta| delta / timelines.slot)
}

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
    async fn per_block_ignores_pre_genesis_blocks() {
        let mut sg = PerBlockSlotGenerator::new(timelines(1000, 10));

        sg.on_new_block(500); // pre-genesis — ignored
        sg.on_new_block(1000); // slot 0

        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(0)));

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
    async fn hybrid_timer_advances_counter_when_no_blocks() {
        let slot_secs = 10;
        let mut sg = HybridSlotGenerator::new(timelines(1000, slot_secs));

        sg.on_new_block(1025); // slot 2
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(2)));

        // One slot_duration elapses — timer fires and emits `last_emitted + 1`.
        tokio::time::advance(Duration::from_secs(slot_secs)).await;
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(3)));

        tokio::time::advance(Duration::from_secs(slot_secs)).await;
        assert_eq!(sg.next().await, Some(SlotEvent::SlotStarted(4)));
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

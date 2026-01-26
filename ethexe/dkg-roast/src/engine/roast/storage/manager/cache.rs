// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use super::types::{ROAST_CACHE_KEEP_ERAS, RoastMessage};
use crate::engine::storage::RoastStore;
use ethexe_common::crypto::{SignAggregate, SignSessionRequest};
use gprimitives::{ActorId, H256};

/// Prunes ROAST caches older than the configured era window.
pub(super) fn prune_caches_if_needed<DB: RoastStore>(db: &DB, era: u64) {
    // Retain a small window of recent eras to keep cache bounded.
    let min_era = era.saturating_sub(ROAST_CACHE_KEEP_ERAS);
    let (sig_removed, nonce_removed) = db.prune_roast_caches_before(min_era);
    if sig_removed > 0 || nonce_removed > 0 {
        tracing::debug!(
            era,
            min_era,
            sig_removed,
            nonce_removed,
            "Pruned ROAST caches"
        );
    }
}

/// Loads cached aggregate signature and wraps it as a network message.
pub(super) fn cached_signature_message<DB: RoastStore>(
    db: &DB,
    era: u64,
    tweak_target: ActorId,
    msg_hash: H256,
) -> Option<RoastMessage> {
    // Fetch cached aggregate signature if present.
    db.signature_cache(era, tweak_target, msg_hash)
        .map(RoastMessage::SignAggregate)
}

/// Convenience wrapper to use request fields for cache lookup.
pub(super) fn cached_signature_for_request<DB: RoastStore>(
    db: &DB,
    request: &SignSessionRequest,
) -> Option<RoastMessage> {
    // Convenience wrapper to fetch cache using request fields.
    cached_signature_message(
        db,
        request.session.era,
        request.tweak_target,
        request.msg_hash,
    )
}

/// Stores aggregate signature in the cache for fast reuse.
pub(super) fn store_aggregate<DB: RoastStore>(
    db: &DB,
    era: u64,
    tweak_target: ActorId,
    msg_hash: H256,
    aggregate: SignAggregate,
) {
    // Persist aggregate for fast reuse in subsequent requests.
    db.set_signature_cache(era, tweak_target, msg_hash, aggregate);
}

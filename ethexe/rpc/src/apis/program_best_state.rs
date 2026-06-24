// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Best-state fan-out for the `program_subscribeBestState` subscription.
//!
//! When the service computes an MB it pushes the `mb_hash` here via
//! [`BestStateManager::notify`]. Each active subscription owns a
//! [`broadcast::Receiver`] and, on every `mb_hash`, loads the MB outcome
//! (cached by hash), picks the transition for its program and forwards a
//! [`ProgramBestState`] to the JSON-RPC sink. The outcome is already persisted
//! by the time `MbComputed` is emitted, so the DB is the source of truth and the
//! cache only avoids repeated reads when many subscribers share an MB.

use super::program::ProgramBestState;
use ethexe_common::{
    db::MbStorageRO,
    gear::{Message, StateTransition},
};
use ethexe_db::Database;
use gprimitives::{ActorId, H160, H256};
use jsonrpsee::{SubscriptionMessage, SubscriptionSink};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::broadcast;
use tracing::{error, trace, warn};

/// Buffered `mb_hash` notifications per subscriber before it starts lagging.
const BROADCAST_CAPACITY: usize = 1024;

/// Maximum number of cached MB outcomes (DB remains the source of truth).
const CACHE_CAPACITY: u64 = 128;

type StateTransitionsCache = moka::sync::Cache<H256, Arc<Vec<StateTransition>>>;
/// Local outcome pre-squashed into a per-program index so each subscriber does
/// a single `BTreeMap` lookup instead of scanning the `Vec` on every MB.
type LocalOutcomeCache = moka::sync::Cache<H256, Arc<BTreeMap<ActorId, Vec<Message>>>>;

/// Cloneable handle shared between [`ProgramApi`](super::ProgramApi) (creates
/// subscribers) and [`RpcService`](crate::RpcService) (pushes `mb_hash`es).
#[derive(Clone)]
pub struct BestStateManager {
    db: Database,
    sender: broadcast::Sender<H256>,
    cache: StateTransitionsCache,
    local_cache: LocalOutcomeCache,
}

impl BestStateManager {
    pub fn new(db: Database) -> Self {
        let (sender, _receiver) = broadcast::channel(BROADCAST_CAPACITY);
        let cache = moka::sync::Cache::builder()
            .max_capacity(CACHE_CAPACITY)
            .build();
        let local_cache = moka::sync::Cache::builder()
            .max_capacity(CACHE_CAPACITY)
            .build();
        Self {
            db,
            sender,
            cache,
            local_cache,
        }
    }

    /// Fan a freshly computed MB out to all active subscribers.
    pub fn notify(&self, mb_hash: H256) {
        // Err only means there are no subscribers right now — nothing to do.
        let _ = self.sender.send(mb_hash);
    }

    fn subscribe(&self) -> broadcast::Receiver<H256> {
        self.sender.subscribe()
    }

    fn outcome(&self, mb_hash: H256) -> Option<Arc<Vec<StateTransition>>> {
        // `try_get_with` coalesces concurrent misses for the same `mb_hash`, so
        // many subscribers sharing an MB trigger only a single DB read.
        self.cache
            .try_get_with(mb_hash, || {
                self.db.mb_outcome(mb_hash).map(Arc::new).ok_or(())
            })
            .ok()
    }

    fn local_outcome(&self, mb_hash: H256) -> Option<Arc<BTreeMap<ActorId, Vec<Message>>>> {
        // Squash the DB `Vec<(ActorId, _)>` into a per-program map once per MB,
        // so each subscriber just does `get(&actor_id)` rather than a linear scan.
        self.local_cache
            .try_get_with(mb_hash, || {
                self.db
                    .mb_local_outcome(mb_hash)
                    .map(|local| Arc::new(local.into_iter().collect()))
                    .ok_or(())
            })
            .ok()
    }
}

/// Spawns the background task driving a single best-state subscription until the
/// client disconnects or the broadcast channel closes.
pub fn spawn_best_state_subscriber(
    sink: SubscriptionSink,
    manager: BestStateManager,
    program_id: H160,
) {
    let actor_id: ActorId = program_id.into();
    let mut receiver = manager.subscribe();

    let _handle = tokio::spawn(async move {
        loop {
            let mb_hash = tokio::select! {
                res = receiver.recv() => match res {
                    Ok(mb_hash) => mb_hash,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "best state subscriber lagged, skipping missed MBs");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                _ = sink.closed() => {
                    trace!("best state subscription closed by user, stop background task");
                    break;
                }
            };

            let Some(transitions) = manager.outcome(mb_hash) else {
                trace!(%mb_hash, "mb outcome not found for best state subscription");
                continue;
            };

            let Some(transition) = transitions.iter().find(|t| t.actor_id == actor_id) else {
                continue;
            };

            // PoC: off-chain (Injected) messages for this program, concatenated
            // into the single `messages` list below. Should be a separate field.
            let local_messages = manager
                .local_outcome(mb_hash)
                .and_then(|local| local.get(&actor_id).cloned())
                .unwrap_or_default();

            let mut messages = transition.messages.clone();
            messages.extend(local_messages);

            let best_state = ProgramBestState {
                mb_hash,
                new_state_hash: transition.new_state_hash,
                messages,
            };

            match SubscriptionMessage::from_json(&best_state) {
                Ok(message) => {
                    if let Err(err) = sink.send(message).await {
                        trace!("failed to send best state, client disconnected: err={err}");
                        break;
                    }
                }
                Err(err) => {
                    error!(
                        ?err,
                        "failed to serialize `ProgramBestState`; this must never happen"
                    );
                }
            }
        }
    });
}

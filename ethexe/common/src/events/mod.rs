// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gprimitives::ActorId;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Events emitted by the Mirror contract (per-program proxy on Ethereum).
pub mod mirror;
/// Events emitted by the Router contract (central co-processor contract on Ethereum).
pub mod router;
/// Events emitted by the WrappedVara ERC-20 token contract.
pub mod wvara;

pub use mirror::{Event as MirrorEvent, RequestEvent as MirrorRequestEvent};
pub use router::{Event as RouterEvent, RequestEvent as RouterRequestEvent};
pub use wvara::Event as WVaraEvent;

/// A decoded on-chain event originating from either a Mirror or the Router contract within a single Ethereum block.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub enum BlockEvent {
    /// An event emitted by the Mirror contract at the given `actor_id`.
    Mirror {
        actor_id: ActorId,
        event: MirrorEvent,
    },
    /// An event emitted by the Router contract.
    Router(RouterEvent),
}

impl BlockEvent {
    /// Constructs a [`BlockEvent::Mirror`] variant from the given actor and event.
    pub fn mirror(actor_id: ActorId, event: MirrorEvent) -> Self {
        Self::Mirror { actor_id, event }
    }

    /// Converts this event into a [`BlockRequestEvent`] if it represents an action that requires processing, returning `None` for purely informational events.
    pub fn to_request(self) -> Option<BlockRequestEvent> {
        Some(match self {
            Self::Mirror { actor_id, event } => BlockRequestEvent::Mirror {
                actor_id,
                event: event.to_request()?,
            },
            Self::Router(event) => BlockRequestEvent::Router(event.to_request()?),
        })
    }
}

impl From<(ActorId, MirrorEvent)> for BlockEvent {
    fn from((actor_id, event): (ActorId, MirrorEvent)) -> Self {
        Self::mirror(actor_id, event)
    }
}

impl From<RouterEvent> for BlockEvent {
    fn from(value: RouterEvent) -> Self {
        Self::Router(value)
    }
}

/// A subset of [`BlockEvent`] containing only events that represent actionable requests requiring computation or state changes.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum BlockRequestEvent {
    /// A request-type event from the Router contract.
    Router(RouterRequestEvent),
    /// A request-type event from the Mirror contract at the given `actor_id`.
    Mirror {
        actor_id: ActorId,
        event: MirrorRequestEvent,
    },
}

impl BlockRequestEvent {
    /// Constructs a [`BlockRequestEvent::Mirror`] variant from the given actor and event.
    pub fn mirror(actor_id: ActorId, event: MirrorRequestEvent) -> Self {
        Self::Mirror { actor_id, event }
    }
}

impl From<(ActorId, MirrorRequestEvent)> for BlockRequestEvent {
    fn from((actor_id, event): (ActorId, MirrorRequestEvent)) -> Self {
        Self::mirror(actor_id, event)
    }
}

impl From<RouterRequestEvent> for BlockRequestEvent {
    fn from(value: RouterRequestEvent) -> Self {
        Self::Router(value)
    }
}

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

use gprimitives::ActorId;
use parity_scale_codec::{Decode, Encode};

mod mirror;
mod router;
mod wvara;

pub use mirror::{Event as MirrorEvent, RequestEvent as MirrorRequestEvent};
pub use router::{Event as RouterEvent, RequestEvent as RouterRequestEvent};
pub use wvara::{Event as WVaraEvent, RequestEvent as WVaraRequestEvent};

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub enum BlockEvent {
    Mirror {
        actor_id: ActorId,
        event: MirrorEvent,
    },
    Router(RouterEvent),
    WVara(WVaraEvent),
}

impl BlockEvent {
    pub fn mirror(actor_id: ActorId, event: MirrorEvent) -> Self {
        Self::Mirror { actor_id, event }
    }

    pub fn to_request(self) -> Option<BlockRequestEvent> {
        Some(match self {
            Self::Mirror { actor_id, event } => BlockRequestEvent::Mirror {
                actor_id,
                event: event.to_request()?,
            },
            Self::Router(event) => BlockRequestEvent::Router(event.to_request()?),
            Self::WVara(event) => BlockRequestEvent::WVara(event.to_request()?),
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

impl From<WVaraEvent> for BlockEvent {
    fn from(value: WVaraEvent) -> Self {
        Self::WVara(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum BlockRequestEvent {
    Router(RouterRequestEvent),
    Mirror {
        actor_id: ActorId,
        event: MirrorRequestEvent,
    },
    WVara(WVaraRequestEvent),
}

impl BlockRequestEvent {
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

impl From<WVaraRequestEvent> for BlockRequestEvent {
    fn from(value: WVaraRequestEvent) -> Self {
        Self::WVara(value)
    }
}

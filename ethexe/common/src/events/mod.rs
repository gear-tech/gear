// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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
use serde::{Deserialize, Serialize};

mod mirror;
mod router;
mod wvara;

pub use mirror::{Event as MirrorEvent, RequestEvent as MirrorRequestEvent};
pub use router::{Event as RouterEvent, RequestEvent as RouterRequestEvent};
pub use wvara::{Event as WVaraEvent, RequestEvent as WVaraRequestEvent};

#[derive(Clone, Debug, Encode, Decode)]
pub enum BlockEvent {
    Mirror {
        address: ActorId,
        event: MirrorEvent,
    },
    Router(RouterEvent),
    WVara(WVaraEvent),
}

impl BlockEvent {
    pub fn mirror(address: ActorId, event: MirrorEvent) -> Self {
        Self::Mirror { address, event }
    }
}

impl From<(ActorId, MirrorEvent)> for BlockEvent {
    fn from((address, event): (ActorId, MirrorEvent)) -> Self {
        Self::mirror(address, event)
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

#[derive(Clone, Debug, Encode, Decode, Serialize, Deserialize)]
pub enum BlockRequestEvent {
    Router(RouterRequestEvent),
    Mirror {
        address: ActorId,
        event: MirrorRequestEvent,
    },
    WVara(WVaraRequestEvent),
}

impl BlockRequestEvent {
    pub fn mirror(address: ActorId, event: MirrorRequestEvent) -> Self {
        Self::Mirror { address, event }
    }
}

impl From<(ActorId, MirrorRequestEvent)> for BlockRequestEvent {
    fn from((address, event): (ActorId, MirrorRequestEvent)) -> Self {
        Self::mirror(address, event)
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

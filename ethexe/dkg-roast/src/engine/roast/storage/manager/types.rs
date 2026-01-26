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

use ethexe_common::{
    Address,
    crypto::{
        SignAggregate, SignCulprits, SignNonceCommit, SignNoncePackage, SignSessionRequest,
        SignShare,
    },
};
use gprimitives::ActorId;
use std::time::Instant;

#[derive(Debug)]
pub(super) struct SessionProgress {
    pub(super) last_activity: Instant,
    pub(super) attempt: u32,
    pub(super) participants: Vec<Address>,
    pub(super) threshold: u16,
    pub(super) tweak_target: ActorId,
    pub(super) leader: Address,
    pub(super) leader_request_seen: bool,
}

pub(super) const ROAST_CACHE_KEEP_ERAS: u64 = 3;

/// Outbound ROAST messages produced by manager/coordinator.
#[derive(Debug, Clone)]
pub enum RoastMessage {
    SignSessionRequest(SignSessionRequest),
    SignNonceCommit(SignNonceCommit),
    SignNoncePackage(SignNoncePackage),
    SignShare(SignShare),
    SignAggregate(SignAggregate),
    SignCulprits(SignCulprits),
}

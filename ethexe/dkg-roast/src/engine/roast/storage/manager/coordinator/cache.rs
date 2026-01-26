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

use super::super::RoastManager;
use crate::engine::storage::RoastStore;
use ethexe_common::crypto::SignAggregate;
use gprimitives::{ActorId, H256};

impl<DB> RoastManager<DB>
where
    DB: RoastStore,
{
    /// Returns completed signature from session state, if any.
    pub fn get_signature(&self, msg_hash: H256, era: u64) -> Option<SignAggregate> {
        self.db
            .sign_session_state(msg_hash, era)
            .and_then(|state| state.aggregate)
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[allow(dead_code)]
    /// Returns cached aggregate signature (test helper).
    pub fn get_cached_signature(
        &self,
        msg_hash: H256,
        era: u64,
        tweak_target: ActorId,
    ) -> Option<SignAggregate> {
        self.db.signature_cache(era, tweak_target, msg_hash)
    }

    #[allow(dead_code)]
    /// Returns the pre-nonce cache for a given era/target.
    pub fn get_pre_nonce_cache(
        &self,
        era: u64,
        tweak_target: ActorId,
    ) -> Option<Vec<ethexe_common::crypto::PreNonceCommitment>> {
        self.db.pre_nonce_cache(era, tweak_target)
    }

    #[allow(dead_code)]
    /// Sets the pre-nonce cache for a given era/target.
    pub fn set_pre_nonce_cache(
        &self,
        era: u64,
        tweak_target: ActorId,
        cache: Vec<ethexe_common::crypto::PreNonceCommitment>,
    ) {
        self.db.set_pre_nonce_cache(era, tweak_target, cache);
    }
}

// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_core::sr25519;
use sp_runtime::impl_opaque_keys;
use sp_std::vec::Vec;

use crate::{Babe, Grandpa, RuntimeBlockWeights, Session};

pub type Migrations = SessionKeysMigration;

impl_opaque_keys! {
    pub struct SessionKeysOld {
        pub babe: Babe,
        pub grandpa: Grandpa,
    }
}

pub struct SessionKeysMigration;

impl OnRuntimeUpgrade for SessionKeysMigration {
    fn on_runtime_upgrade() -> Weight {
        Session::upgrade_keys::<SessionKeysOld, _>(|_id, keys| crate::SessionKeys {
            babe: keys.babe.clone(),
            grandpa: keys.grandpa,
            im_online: pallet_im_online::sr25519::AuthorityId::from(sr25519::Public::from(
                keys.babe.clone(),
            )),
            authority_discovery: sp_authority_discovery::AuthorityId::from(sr25519::Public::from(
                keys.babe,
            )),
        });

        RuntimeBlockWeights::get().max_block
    }
}

// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use gear_common::Origin;
use gear_runtime::Runtime;
use pallet_gear::BlockGasLimitOf;
use runtime_primitives::{AccountId, AccountPublic, Balance};
use sp_consensus_babe::AuthorityId as BabeId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{sr25519::Public, Pair, Public as TPublic};
use sp_runtime::{app_crypto::UncheckedFrom, traits::IdentifyAccount};

pub fn alice() -> AccountId {
    sp_keyring::Sr25519Keyring::Alice.to_account_id()
}

/// Get account from [`gear_common::Origin`] implementor.
pub fn account<T: Origin>(v: T) -> AccountId {
    AccountId::unchecked_from(v.into_origin())
}

// TODO #2307 BabeId and GrandpaId are not needed at first?
/// Generate authority keys.
pub fn authority_keys_from_seed(s: &str) -> (AccountId, BabeId, GrandpaId) {
    (
        get_acc_id_from_seed::<Public>(s),
        get_pub_key_from_seed::<BabeId>(s),
        get_pub_key_from_seed::<GrandpaId>(s),
    )
}

/// Generate an account ID from seed.
pub fn get_acc_id_from_seed<T: TPublic>(seed: &str) -> AccountId
where
    AccountPublic: From<<T::Pair as Pair>::Public>,
{
    AccountPublic::from(get_pub_key_from_seed::<T>(seed)).into_account()
}

// Generate a crypto pair from seed.
pub fn get_pub_key_from_seed<T: TPublic>(seed: &str) -> <T::Pair as Pair>::Public {
    T::Pair::from_string(&format!("//{}", seed), None)
        .expect("static values are valid; qed")
        .public()
}

pub fn acc_max_balance() -> Balance {
    BlockGasLimitOf::<Runtime>::get().saturating_mul(20) as u128
}

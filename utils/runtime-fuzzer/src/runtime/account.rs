// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use crate::{
    data::FulfilledDataRequirement,
    generator::{GearCallsGenerator, AUXILIARY_SIZE},
    runtime,
};
use gear_common::{Gas, Origin};
use gear_wasm_gen::{Result, Unstructured};
use pallet_balances::Pallet as BalancesPallet;
use pallet_gear::BlockGasLimitOf;
use pallet_gear_bank::Config as GearBankConfig;
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use runtime_primitives::{AccountId, AccountPublic, Balance};
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{sr25519::Public, Pair, Public as TPublic};
use sp_runtime::{app_crypto::UncheckedFrom, traits::IdentifyAccount};
use std::mem;
use vara_runtime::{Runtime, EXISTENTIAL_DEPOSIT};

pub fn alice() -> AccountId {
    sp_keyring::Sr25519Keyring::Alice.to_account_id()
}

/// Get account from [`gear_common::Origin`] implementor.
pub fn account<T: Origin>(v: T) -> AccountId {
    AccountId::unchecked_from(v.into_origin())
}

// TODO #2307 BabeId and GrandpaId are not needed at first?
/// Generate authority keys.
pub fn authority_keys_from_seed(
    s: &str,
) -> (
    AccountId,
    BabeId,
    GrandpaId,
    ImOnlineId,
    AuthorityDiscoveryId,
) {
    (
        get_acc_id_from_seed::<Public>(s),
        get_pub_key_from_seed::<BabeId>(s),
        get_pub_key_from_seed::<GrandpaId>(s),
        get_pub_key_from_seed::<ImOnlineId>(s),
        get_pub_key_from_seed::<AuthorityDiscoveryId>(s),
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

pub fn acc_max_balance_gas() -> Gas {
    BlockGasLimitOf::<Runtime>::get().saturating_mul(20)
}

pub fn gas_to_value(gas: Gas) -> Balance {
    <Runtime as GearBankConfig>::GasMultiplier::get().gas_to_value(gas)
}

pub struct BalanceManager<'a> {
    unstructured: Unstructured<'a>,
    pub sender: AccountId,
}

impl<'a> BalanceManager<'a> {
    pub(crate) fn new(
        account: AccountId,
        data_requirement: FulfilledDataRequirement<'a, Self>,
    ) -> Self {
        Self {
            sender: account,
            unstructured: Unstructured::new(data_requirement.data),
        }
    }

    pub(crate) fn update_balance(&mut self) -> Result<BalanceState> {
        let max_balance = runtime::gas_to_value(runtime::acc_max_balance_gas());
        let new_balance = self
            .unstructured
            .int_in_range(EXISTENTIAL_DEPOSIT..=max_balance)?;

        runtime::set_balance(self.sender.clone(), new_balance)
            .unwrap_or_else(|e| unreachable!("Balance update failed: {e:?}"));
        assert_eq!(
            new_balance,
            BalancesPallet::<Runtime>::free_balance(&self.sender),
            "internal error: new balance set logic is corrupted."
        );
        log::info!("Current balance of the sender - {new_balance}.");

        Ok(BalanceState(new_balance))
    }
}

impl BalanceManager<'_> {
    pub(crate) const fn random_data_requirement() -> usize {
        const VALUE_SIZE: usize = mem::size_of::<u128>();

        VALUE_SIZE
            * (GearCallsGenerator::MAX_UPLOAD_PROGRAM_CALLS
                + GearCallsGenerator::MAX_SEND_MESSAGE_CALLS
                + GearCallsGenerator::MAX_SEND_REPLY_CALLS)
            + AUXILIARY_SIZE
    }
}

pub struct BalanceState(Balance);

impl BalanceState {
    pub(crate) fn into_inner(self) -> Balance {
        self.0
    }
}

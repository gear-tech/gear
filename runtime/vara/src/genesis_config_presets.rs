// This file is part of Gear.

// Copyright (C) 2025 Gear Technologies Inc.
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

/// Configure initial storage state for FRAME modules.
pub fn testnet_genesis(
    initial_authorities: Vec<(
        AccountId,
        AccountId,
        BabeId,
        GrandpaId,
        ImOnlineId,
        AuthorityDiscoveryId,
    )>,
    root_key: AccountId,
    endowed_accounts: Vec<AccountId>,
    bank_account: AccountId,
    _enable_println: bool,
) -> serde_json::Value {
    const ENDOWMENT: u128 = 1_000_000 * TOKEN;
    const STASH: u128 = 100 * TOKEN;
    const MIN_NOMINATOR_BOND: u128 = 50 * TOKEN;

    let _num_endowed_accounts = endowed_accounts.len();

    let mut balances = endowed_accounts
        .iter()
        .map(|k: &AccountId| (k.clone(), ENDOWMENT))
        .chain(initial_authorities.iter().map(|x| (x.0.clone(), STASH)))
        .collect::<Vec<_>>();

    balances.push((bank_account, EXISTENTIAL_DEPOSIT));

    serde_json::json!({
        "balances": {
            "balances": balances,
        },

        "session": {
            "keys": initial_authorities
                    .iter()
                    .map(|x| {
                        (
                            x.0.clone(),
                            x.0.clone(),
                            session_keys(x.2.clone(), x.3.clone(), x.4.clone(), x.5.clone()),
                        )
                    })
                    .collect::<Vec<_>>(),
        },
        "staking": {
            "validatorCount": initial_authorities.len() as u32,
            "minimumValidatorCount": 4,
            "stakers": initial_authorities
            .iter()
            .map(|x| (x.0.clone(), x.1.clone(), STASH, StakerStatus::<AccountId>::Validator))
            .collect::<Vec<_>>(),
            "invulnerables": initial_authorities.iter().map(|x| x.0.clone()).collect::<Vec<_>>(),
            "forceEra": Forcing::ForceNone,
            "slashRewardFraction": Perbill::from_percent(10),
            "minNominatorBond": MIN_NOMINATOR_BOND,
        },
        "nominationPools": {
            "minCreateBond": 10 * ECONOMIC_UNITS,
            "minJoinBond": ECONOMIC_UNITS,
        },
        "stakingRewards": {
            // 41.08%
            "nonStakeable": Perquintill::from_rational(4_108u64, 10_000u64),
            // 85%
            "idealStake": Perquintill::from_percent(85),
            // 5.78%
            "targetInflation": Perquintill::from_rational(578u64, 10_000u64),
        },
        "babe": {
            "epochConfig": Some(vara_runtime::BABE_GENESIS_EPOCH_CONFIG),
        },
        "sudo": {
            // Assign network admin rights.
            "key": Some(root_key),
        },
    })
}

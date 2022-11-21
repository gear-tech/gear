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

use cfg_if::cfg_if;
use codec::{Decode, Encode};
use frame_support::{
    traits::{GenesisBuild, OnFinalize, OnInitialize, StorePreimage},
    BasicExternalities,
};
use frame_system as system;
pub use once_cell::sync::Lazy;
use pallet_democracy::{AccountVote, Conviction, Vote};
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
pub use parking_lot::RwLock;
use rand::distributions::{Alphanumeric, DistString};
use runtime_primitives::AccountPublic;
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::{
    digests::{PreDigest, SecondaryPlainPreDigest},
    AuthorityId as BabeId, Slot, BABE_ENGINE_ID,
};
pub use sp_core::offchain::testing::PoolState;
use sp_core::{
    offchain::{
        testing::{TestOffchainExt, TestTransactionPoolExt},
        OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
    },
    sr25519, Pair, Public,
};
use sp_finality_grandpa::AuthorityId as GrandpaId;
pub use sp_runtime::AccountId32;
use sp_runtime::{traits::IdentifyAccount, Digest, DigestItem, MultiAddress, Perbill};
use sp_std::prelude::*;
pub use std::sync::Arc;
use system::pallet_prelude::BlockNumberFor;

cfg_if! {
    if #[cfg(feature = "use-vara-runtime")] {
        pub use vara_runtime::{
            constants::currency::{DOLLARS, UNITS as TOKEN},
            AuthorityDiscoveryConfig, Authorship, Babe, BabeConfig, Balances, CooloffPeriod, CouncilConfig,
            Democracy, DemocracyConfig, DesiredMembers, ElectionsConfig, EnactmentPeriod, EpochDuration,
            FastTrackVotingPeriod, Grandpa, GrandpaConfig, ImOnlineConfig, LaunchPeriod,
            NominationPoolsConfig, Preimage, Runtime, RuntimeCall, RuntimeOrigin, Scheduler, Session,
            SessionConfig, SessionKeys, StakerStatus, Staking, StakingConfig, SudoConfig, System,
            TechnicalCommitteeConfig, Timestamp, TransactionPayment, TransactionPaymentConfig, Treasury,
            TreasuryConfig, UncheckedExtrinsic, VotingPeriod, BABE_GENESIS_EPOCH_CONFIG,
            EXISTENTIAL_DEPOSIT, MILLISECS_PER_BLOCK,
        };
    } else {
        pub use crate::runtime::{
            constants::currency::{DOLLARS, UNITS as TOKEN},
            AuthorityDiscoveryConfig, Authorship, Babe, BabeConfig, Balances, CooloffPeriod, CouncilConfig,
            Democracy, DemocracyConfig, DesiredMembers, ElectionsConfig, EnactmentPeriod, EpochDuration,
            FastTrackVotingPeriod, Grandpa, GrandpaConfig, ImOnlineConfig, LaunchPeriod,
            NominationPoolsConfig, Preimage, Runtime, RuntimeCall, RuntimeOrigin, Scheduler, Session,
            SessionConfig, SessionKeys, StakerStatus, Staking, StakingConfig, SudoConfig, System,
            TechnicalCommitteeConfig, Timestamp, TransactionPayment, TransactionPaymentConfig, Treasury,
            TreasuryConfig, UncheckedExtrinsic, VotingPeriod, BABE_GENESIS_EPOCH_CONFIG,
            EXISTENTIAL_DEPOSIT, MILLISECS_PER_BLOCK,
        };
    }
}

pub type ValidatorAccountId = (
    AccountId32, // stash
    AccountId32, // controller
    BabeId,
    GrandpaId,
    ImOnlineId,
    AuthorityDiscoveryId,
);

pub type NominatorAccountId = (AccountId32, AccountId32);

pub static ROOT_KEY: Lazy<AccountId32> =
    Lazy::new(|| get_account_id_from_seed::<sr25519::Public>("root"));
pub static SIGNING_KEY: Lazy<AccountId32> =
    Lazy::new(|| get_account_id_from_seed::<sr25519::Public>("signing"));

// Generate a crypto pair from seed.
pub(crate) fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
    TPublic::Pair::from_string(&format!("//{seed}"), None)
        .expect("static values are valid; qed")
        .public()
}

// Generate an account ID from seed.
pub(crate) fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId32
where
    AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
    AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

pub fn authority_keys_from_seed(s: &str) -> ValidatorAccountId {
    (
        get_account_id_from_seed::<sr25519::Public>(&format!("{s}//stash")),
        get_account_id_from_seed::<sr25519::Public>(s),
        get_from_seed::<BabeId>(s),
        get_from_seed::<GrandpaId>(s),
        get_from_seed::<ImOnlineId>(s),
        get_from_seed::<AuthorityDiscoveryId>(s),
    )
}

pub fn nominator_keys_from_seed(s: &str) -> NominatorAccountId {
    (
        get_account_id_from_seed::<sr25519::Public>(&format!("{s}//stash")),
        get_account_id_from_seed::<sr25519::Public>(s),
    )
}

pub fn generate_random_authorities(num_authorities: usize) -> Vec<ValidatorAccountId> {
    let mut initial_authorities = Vec::new();
    for _i in 0..num_authorities {
        let seed = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
        initial_authorities.push(authority_keys_from_seed(&seed));
    }
    initial_authorities
}

pub(crate) fn initialize(new_blk: BlockNumberFor<Runtime>) {
    log::debug!("📦 Initializing block {}", new_blk);

    // Emulate uniform distribution of blocks authors by appointing
    // an author in a round-robin manner
    let authority_index = new_blk % Staking::validator_count().max(1);
    let slot = Slot::from(u64::from(new_blk));
    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(
            BABE_ENGINE_ID,
            PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
                slot,
                authority_index,
            })
            .encode(),
        )],
    };

    System::initialize(&new_blk, &System::parent_hash(), &pre_digest);
    System::set_block_number(new_blk);
}

// Run on_initialize hooks in order as they appear in AllPalletsWithSystem.
pub(crate) fn on_initialize(new_block_number: BlockNumberFor<Runtime>) {
    System::on_initialize(new_block_number);
    Babe::on_initialize(new_block_number);
    Timestamp::set_timestamp(new_block_number.saturating_mul(2000) as u64);
    Balances::on_initialize(new_block_number);
    TransactionPayment::on_initialize(new_block_number);
    Authorship::on_initialize(new_block_number);
    Session::on_initialize(new_block_number);
    Staking::on_initialize(new_block_number);
    Scheduler::on_initialize(new_block_number);
    Democracy::on_initialize(new_block_number);
}

// Run on_finalize hooks (in pallets reverse order, as they appear in AllPalletsWithSystem)
pub(crate) fn on_finalize(current_blk: BlockNumberFor<Runtime>) {
    Staking::on_finalize(current_blk);
    Authorship::on_finalize(current_blk);
    TransactionPayment::on_finalize(current_blk);
    Balances::on_finalize(current_blk);
    Grandpa::on_finalize(current_blk);
    Babe::on_finalize(current_blk);
    System::on_finalize(current_blk);
}

#[derive(Default)]
pub struct ExtBuilder {
    seed: u32,
    initial_authorities: Vec<ValidatorAccountId>,
    stash: u128,
    endowed_accounts: Vec<AccountId32>,
    endowment: u128,
}

impl ExtBuilder {
    pub fn seed(mut self, s: u32) -> Self {
        self.seed = s;
        self
    }

    cfg_if! {
        if #[cfg(feature = "use-vara-runtime")] {
            pub fn epoch_duration(self, _p: u64) -> Self {
                // Do nothing: EpochDuration is a const
                self
            }
        } else {
            pub fn epoch_duration(self, p: u64) -> Self {
                <EpochDuration>::set(p);
                // Change all related durations accordinly
                <LaunchPeriod>::set(p as u32 * 24_u32);
                <VotingPeriod>::set(p as u32 * 24_u32);
                <FastTrackVotingPeriod>::set(p as u32 * 6_u32);
                <EnactmentPeriod>::set(p as u32 * 25_u32);
                <CooloffPeriod>::set(p as u32 * 24_u32);
                self
            }
        }
    }

    pub fn stash(mut self, s: u128) -> Self {
        self.stash = s;
        self
    }

    pub fn endowment(mut self, e: u128) -> Self {
        self.endowment = e;
        self
    }

    pub fn initial_authorities(mut self, authorities: Vec<ValidatorAccountId>) -> Self {
        self.initial_authorities = authorities;
        self
    }

    pub fn endowed_accounts(mut self, accounts: Vec<AccountId32>) -> Self {
        self.endowed_accounts = accounts;
        self
    }

    pub fn build(self) -> sp_io::TestExternalities {
        let mut storage = system::GenesisConfig::default()
            .build_storage::<Runtime>()
            .unwrap();

        let balances: Vec<(AccountId32, u128)> = self
            .initial_authorities
            .iter()
            .map(|x| (x.0.clone(), self.stash))
            .chain(
                self.endowed_accounts
                    .iter()
                    .map(|k| (k.clone(), self.endowment)),
            )
            .collect();

        pallet_balances::GenesisConfig::<Runtime> { balances }
            .assimilate_storage(&mut storage)
            .unwrap();

        #[allow(clippy::unnecessary_operation)]
        BasicExternalities::execute_with_storage(&mut storage, || {
            BabeConfig {
                authorities: Default::default(),
                epoch_config: Some(BABE_GENESIS_EPOCH_CONFIG),
            };

            GrandpaConfig::default();

            TransactionPaymentConfig::default();

            DemocracyConfig::default();
            CouncilConfig::default();
            TreasuryConfig::default();
            AuthorityDiscoveryConfig::default();
        });

        SessionConfig {
            keys: self
                .initial_authorities
                .iter()
                .map(|x| {
                    (
                        x.0.clone(),
                        x.0.clone(),
                        SessionKeys {
                            babe: x.2.clone(),
                            grandpa: x.3.clone(),
                            im_online: x.4.clone(),
                            authority_discovery: x.5.clone(),
                        },
                    )
                })
                .collect(),
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        StakingConfig {
            validator_count: self.initial_authorities.len() as u32,
            minimum_validator_count: self.initial_authorities.len() as u32,
            stakers: self
                .initial_authorities
                .iter()
                .map(|x| {
                    (
                        x.0.clone(),
                        x.1.clone(),
                        self.stash,
                        StakerStatus::<AccountId32>::Validator,
                    )
                })
                .collect::<Vec<_>>(),
            invulnerables: self
                .initial_authorities
                .iter()
                .map(|x| x.0.clone())
                .collect(),
            slash_reward_fraction: Perbill::from_percent(10),
            ..Default::default()
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        let num_endowed_accounts = self.endowed_accounts.len();
        ElectionsConfig {
            members: self.endowed_accounts
                [0..((num_endowed_accounts + 1) / 2).min(DesiredMembers::get() as usize)]
                .iter()
                .map(|member| (member.clone(), self.stash))
                .collect(),
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        TechnicalCommitteeConfig {
            members: self.endowed_accounts[0..(num_endowed_accounts + 1) / 2].to_vec(),
            phantom: Default::default(),
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        SudoConfig {
            key: Some((*ROOT_KEY).clone()),
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        ImOnlineConfig { keys: vec![] }
            .assimilate_storage(&mut storage)
            .unwrap();

        NominationPoolsConfig {
            min_create_bond: 10 * DOLLARS,
            min_join_bond: DOLLARS,
            ..Default::default()
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        let mut ext: sp_io::TestExternalities = storage.into();

        ext.execute_with(|| {
            let new_blk = 1;
            initialize(new_blk);
            on_initialize(new_blk);
        });
        ext
    }

    pub fn build_with_offchain(self) -> (sp_io::TestExternalities, Arc<RwLock<PoolState>>) {
        let mut ext = self.build();
        let (offchain, _) = TestOffchainExt::new();
        let (pool, pool_state) = TestTransactionPoolExt::new();

        ext.register_extension(OffchainDbExt::new(offchain.clone()));
        ext.register_extension(OffchainWorkerExt::new(offchain));
        ext.register_extension(TransactionPoolExt::new(pool));

        (ext, pool_state)
    }
}

#[allow(unused)]
pub(crate) fn run_to_block(n: u32, remaining_weight: Option<u64>) {
    while System::block_number() < n {
        let current_blk = System::block_number();
        on_finalize(current_blk);

        let new_block_number = current_blk + 1;
        initialize(new_block_number);
        on_initialize(new_block_number);
    }
}

pub(crate) fn run_for_n_blocks(n: u32) {
    let now = System::block_number();
    let until = now + n;
    for current_blk in now..until {
        on_finalize(current_blk);

        let new_block_number = current_blk + 1;
        initialize(new_block_number);
        on_initialize(new_block_number);
    }
}

pub(crate) fn run_to_block_with_offchain(n: u32, pool: &Arc<RwLock<PoolState>>) {
    let now = System::block_number();
    for i in now..n {
        // Processing extrinsics in current block, if pool supplied
        process_tx_pool(pool);
        log::debug!("✅ Done processing transaction pool at block {}", i);

        on_finalize(i);

        let new_blk = i + 1;
        initialize(new_blk);
        on_initialize(new_blk);
    }
}

#[allow(unused)]
pub(crate) fn run_for_n_blocks_with_offchain(n: u32, pool: &Arc<RwLock<PoolState>>) {
    let now = System::block_number();
    let until = now + n;
    for i in now..until {
        // Processing extrinsics in current block, if pool supplied
        process_tx_pool(pool);
        log::debug!("✅ Done processing transaction pool at block {}", i);

        on_finalize(i);

        let new_blk = i + 1;
        initialize(new_blk);
        on_initialize(new_blk);
    }
}

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

pub(crate) fn process_tx_pool(pool: &Arc<RwLock<PoolState>>) {
    let mut guard = pool.write();
    guard.transactions.iter().cloned().for_each(|bytes| {
        let _tx = UncheckedExtrinsic::decode(&mut &bytes[..]).unwrap();
    });
    guard.transactions = vec![];
}

pub fn set_balance_proposal(
    account_id: AccountId32,
    value: u128,
) -> pallet_democracy::BoundedCallOf<Runtime> {
    let inner = pallet_balances::Call::set_balance {
        who: MultiAddress::Id(account_id),
        new_free: value,
        new_reserved: 0,
    };
    let outer = RuntimeCall::Balances(inner);
    Preimage::bound(outer).unwrap()
}

pub fn vote_aye(amount: u128) -> AccountVote<u128> {
    AccountVote::Standard {
        vote: Vote {
            aye: true,
            conviction: Conviction::Locked1x,
        },
        balance: amount,
    }
}

pub fn vote_nay(amount: u128) -> AccountVote<u128> {
    AccountVote::Standard {
        vote: Vote {
            aye: false,
            conviction: Conviction::Locked1x,
        },
        balance: amount,
    }
}

pub(crate) fn validators_total_balance(validators: Vec<ValidatorAccountId>) -> u128 {
    validators
        .iter()
        .map(|(stash_id, _, _, _, _, _)| Balances::free_balance(stash_id))
        .fold(0_u128, |acc, v| acc.saturating_add(v))
}

pub(crate) fn nominators_total_balance(nominators: Vec<NominatorAccountId>) -> u128 {
    nominators
        .iter()
        .map(|(stash_id, _)| Balances::free_balance(stash_id))
        .fold(0_u128, |acc, v| acc.saturating_add(v))
}

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

use codec::{Decode, Encode};
use common::{
    storage::{IterableMap, Messenger},
    GasTree,
};
use frame_support::{
    traits::{GenesisBuild, OnFinalize, OnIdle, OnInitialize},
    BasicExternalities,
};
use frame_system as system;
use gear_core::ids::{CodeId, ProgramId};
#[cfg(feature = "gear-native")]
use gear_runtime::{
    Aura, AuraConfig, Authorship, Balances, Gear, GearGas, GearMessenger, GearPayment, GearProgram,
    Grandpa, GrandpaConfig, Runtime, SudoConfig, System, TransactionPayment,
    TransactionPaymentConfig, UncheckedExtrinsic,
};
use pallet_gear::{BlockGasLimitOf, GasHandlerOf};
use pallet_gear_gas::Error as GasError;
use parking_lot::RwLock;
use rand::{rngs::StdRng, RngCore};
use runtime_primitives::AccountPublic;
#[cfg(feature = "authoring-aura")]
use sp_consensus_aura::{sr25519::AuthorityId as AuraId, AURA_ENGINE_ID};
#[cfg(not(feature = "authoring-aura"))]
use sp_consensus_babe::{
    digests::{PreDigest, SecondaryPlainPreDigest},
    AuthorityId as BabeId, BABE_ENGINE_ID,
};
use sp_consensus_slots::Slot;
use sp_core::{
    offchain::{
        testing::{PoolState, TestOffchainExt, TestTransactionPoolExt},
        OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
    },
    sr25519, Pair, Public,
};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use sp_runtime::{traits::IdentifyAccount, AccountId32, Digest, DigestItem};
use sp_std::collections::btree_map::BTreeMap;
use std::sync::Arc;
use system::pallet_prelude::BlockNumberFor;
#[cfg(all(not(feature = "gear-native"), feature = "vara-native"))]
use vara_runtime::{
    Authorship, Babe, BabeConfig, Balances, Gear, GearGas, GearMessenger, GearPayment, GearProgram,
    Grandpa, GrandpaConfig, Runtime, Session, SessionConfig, SessionKeys, SudoConfig, System,
    TransactionPayment, TransactionPaymentConfig, UncheckedExtrinsic,
};

type GasNodeKeyOf<T> = <GasHandlerOf<T> as GasTree>::Key;
type GasBalanceOf<T> = <GasHandlerOf<T> as GasTree>::Balance;

pub(crate) type WaitlistOf<T> = <<T as pallet_gear::Config>::Messenger as Messenger>::Waitlist;
pub(crate) type MailboxOf<T> = <<T as pallet_gear::Config>::Messenger as Messenger>::Mailbox;

// Generate a crypto pair from seed.
pub(crate) fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
    TPublic::Pair::from_string(&format!("//{}", seed), None)
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

#[cfg(feature = "authoring-aura")]
type AuthorityId = AuraId;
#[cfg(not(feature = "authoring-aura"))]
type AuthorityId = BabeId;

// Generate authority keys.
pub(crate) fn authority_keys_from_seed(s: &str) -> (AccountId32, AuthorityId, GrandpaId) {
    (
        get_account_id_from_seed::<sr25519::Public>(s),
        get_from_seed::<AuthorityId>(s),
        get_from_seed::<GrandpaId>(s),
    )
}

pub(crate) fn create_random_accounts(
    rng: &mut StdRng,
    root_acc: &AccountId32,
) -> Vec<(AccountId32, u128)> {
    let initial_accounts_num = 1 + (rng.next_u32() % 1000); // [1..1000]
    let mut accounts = vec![(root_acc.clone(), 1_000_000_000_000_000_u128)];
    for _ in 1..initial_accounts_num {
        let mut acc_id = [0_u8; 32];
        rng.fill_bytes(&mut acc_id);
        let balance = (rng.next_u64() >> 14) as u128; // approx. up to 10^15
        accounts.push((acc_id.into(), balance));
    }
    accounts
}

pub(crate) fn initialize(new_blk: BlockNumberFor<Runtime>) {
    log::debug!("📦 Initializing block {}", new_blk);

    // All blocks are to be authored by validator at index 0
    let slot = Slot::from(u64::from(new_blk));
    #[cfg(feature = "authoring-aura")]
    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode())],
    };
    #[cfg(not(feature = "authoring-aura"))]
    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(
            BABE_ENGINE_ID,
            PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
                slot,
                authority_index: 0,
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
    #[cfg(feature = "authoring-aura")]
    Aura::on_initialize(new_block_number);
    #[cfg(not(feature = "authoring-aura"))]
    Babe::on_initialize(new_block_number);
    Balances::on_initialize(new_block_number);
    TransactionPayment::on_initialize(new_block_number);
    Authorship::on_initialize(new_block_number);
    GearProgram::on_initialize(new_block_number);
    GearMessenger::on_initialize(new_block_number);
    Gear::on_initialize(new_block_number);
    GearGas::on_initialize(new_block_number);
    #[cfg(not(feature = "authoring-aura"))]
    Session::on_initialize(new_block_number);
}

// Run on_finalize hooks (in pallets reverse order, as they appear in AllPalletsWithSystem)
pub(crate) fn on_finalize(current_blk: BlockNumberFor<Runtime>) {
    GearPayment::on_finalize(current_blk);
    GearGas::on_finalize(current_blk);
    Gear::on_finalize(current_blk);
    GearMessenger::on_finalize(current_blk);
    GearProgram::on_finalize(current_blk);
    Authorship::on_finalize(current_blk);
    TransactionPayment::on_finalize(current_blk);
    Balances::on_finalize(current_blk);
    Grandpa::on_finalize(current_blk);
    #[cfg(not(feature = "authoring-aura"))]
    Babe::on_finalize(current_blk);
    System::on_finalize(current_blk);
}

pub(crate) fn new_test_ext(
    balances: Vec<(impl Into<AccountId32>, u128)>,
    seeds: Vec<&str>,
    root_key: AccountId32,
) -> sp_io::TestExternalities {
    assert!(!seeds.is_empty());

    #[cfg(feature = "authoring-aura")]
    let initial_authorities: Vec<(AccountId32, AuraId, GrandpaId)> =
        seeds.into_iter().map(authority_keys_from_seed).collect();
    #[cfg(not(feature = "authoring-aura"))]
    let initial_authorities: Vec<(AccountId32, BabeId, GrandpaId)> = seeds
        .into_iter()
        .map(|s| authority_keys_from_seed(s))
        .collect();

    let mut t = system::GenesisConfig::default()
        .build_storage::<Runtime>()
        .unwrap();

    pallet_balances::GenesisConfig::<Runtime> {
        balances: balances
            .into_iter()
            .map(|(acc, balance)| (acc.into(), balance))
            .chain(
                initial_authorities
                    .iter()
                    .cloned()
                    .map(|(acc, _, _)| (acc, 1000)),
            )
            .collect(),
    }
    .assimilate_storage(&mut t)
    .unwrap();

    #[cfg(feature = "authoring-aura")]
    AuraConfig {
        authorities: initial_authorities
            .iter()
            .cloned()
            .map(|(_, x, _)| x)
            .collect(),
    }
    .assimilate_storage(&mut t)
    .unwrap();

    BasicExternalities::execute_with_storage(&mut t, || {
        #[cfg(not(feature = "authoring-aura"))]
        BabeConfig {
            authorities: Default::default(),
            epoch_config: Some(vara_runtime::BABE_GENESIS_EPOCH_CONFIG),
        };

        #[cfg(feature = "authoring-aura")]
        <GrandpaConfig as GenesisBuild<Runtime>>::build(&GrandpaConfig {
            authorities: initial_authorities.into_iter().map(|x| (x.2, 1)).collect(),
        });
        #[cfg(not(feature = "authoring-aura"))]
        GrandpaConfig {
            authorities: Default::default(),
        };

        <TransactionPaymentConfig as GenesisBuild<Runtime>>::build(
            &TransactionPaymentConfig::default(),
        );
    });

    #[cfg(not(feature = "authoring-aura"))]
    SessionConfig {
        keys: initial_authorities
            .iter()
            .map(|x| {
                (
                    x.0.clone(),
                    x.0.clone(),
                    SessionKeys {
                        babe: x.1.clone(),
                        grandpa: x.2.clone(),
                    },
                )
            })
            .collect::<Vec<_>>(),
    }
    .assimilate_storage(&mut t)
    .unwrap();

    SudoConfig {
        key: Some(root_key),
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext: sp_io::TestExternalities = t.into();

    ext.execute_with(|| {
        let new_blk = 1;
        initialize(new_blk);
        on_initialize(new_blk);
    });
    ext
}

pub(crate) fn with_offchain_ext(
    balances: Vec<(impl Into<AccountId32>, u128)>,
    seeds: Vec<&str>,
    root_key: AccountId32,
) -> (sp_io::TestExternalities, Arc<RwLock<PoolState>>) {
    let mut ext = new_test_ext(balances, seeds, root_key);
    let (offchain, _) = TestOffchainExt::new();
    let (pool, pool_state) = TestTransactionPoolExt::new();

    ext.register_extension(OffchainDbExt::new(offchain.clone()));
    ext.register_extension(OffchainWorkerExt::new(offchain));
    ext.register_extension(TransactionPoolExt::new(pool));

    (ext, pool_state)
}

#[allow(unused)]
pub(crate) fn run_to_block(n: u32, remaining_weight: Option<u64>) {
    while System::block_number() < n {
        // Run on_idle hook that processes the queue
        let remaining_weight = remaining_weight.unwrap_or_else(BlockGasLimitOf::<Runtime>::get);
        Gear::on_idle(System::block_number(), remaining_weight);

        let current_blk = System::block_number();
        on_finalize(current_blk);

        let new_block_number = current_blk + 1;
        initialize(new_block_number);
        on_initialize(new_block_number);
    }
}

pub(crate) fn run_to_block_with_ocw(
    n: u32,
    pool: &Arc<RwLock<PoolState>>,
    remaining_weight: Option<u64>,
) {
    let now = System::block_number();
    for i in now..n {
        // Processing extrinsics in current block, if pool supplied
        process_tx_pool(pool);
        log::debug!("✅ Done processing transaction pool at block {}", i);
        let remaining_weight = remaining_weight.unwrap_or_else(BlockGasLimitOf::<Runtime>::get);

        // Processing message queue
        Gear::on_idle(i, remaining_weight);

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

pub(crate) fn generate_program_id(code: &[u8], salt: &[u8]) -> ProgramId {
    ProgramId::generate(CodeId::generate(code), salt)
}

pub(crate) fn process_tx_pool(pool: &Arc<RwLock<PoolState>>) {
    let mut guard = pool.write();
    guard.transactions.iter().cloned().for_each(|bytes| {
        let _tx = UncheckedExtrinsic::decode(&mut &bytes[..]).unwrap();
    });
    guard.transactions = vec![];
}

pub(crate) fn total_gas_in_wait_list() -> u64 {
    let mut total_lock = 0;

    // Iterate through the wait list and record the respective gas nodes value limits
    // attributing the latter to the nearest `node_with_value` ID to avoid duplication
    let gas_limit_by_node_id: BTreeMap<GasNodeKeyOf<Runtime>, GasBalanceOf<Runtime>> =
        WaitlistOf::<Runtime>::iter()
            .map(|(dispatch, _)| {
                let node_id = dispatch.id();
                let (value, ancestor_id) = GasHandlerOf::<Runtime>::get_limit_node(node_id)
                    .expect("There is always a value node for a valid dispatch ID");

                let lock = GasHandlerOf::<Runtime>::get_lock(node_id).unwrap_or_else(|e| {
                    if e == GasError::<Runtime>::Forbidden.into() {
                        0
                    } else {
                        panic!("GasTree error: {:?}", e)
                    }
                });

                total_lock += lock;

                (ancestor_id, value)
            })
            .collect();

    gas_limit_by_node_id
        .into_iter()
        .fold(0_u64, |acc, (_, val)| acc + val)
        .saturating_add(total_lock)
}

pub(crate) fn total_gas_in_mailbox() -> u64 {
    let mut total_lock = 0;

    // Iterate through the mailbox and record the respective gas nodes value limits
    // attributing the latter to the nearest `node_with_value` ID to avoid duplication
    let gas_limit_by_node_id: BTreeMap<GasNodeKeyOf<Runtime>, GasBalanceOf<Runtime>> =
        MailboxOf::<Runtime>::iter()
            .map(|(dispatch, _)| {
                let node_id = dispatch.id();
                let (value, ancestor_id) = GasHandlerOf::<Runtime>::get_limit_node(node_id)
                    .expect("There is always a value node for a valid dispatch ID");

                let lock = GasHandlerOf::<Runtime>::get_lock(node_id).unwrap_or_else(|e| {
                    if e == GasError::<Runtime>::Forbidden.into() {
                        0
                    } else {
                        panic!("GasTree error: {:?}", e)
                    }
                });

                total_lock += lock;

                (ancestor_id, value)
            })
            .collect();

    gas_limit_by_node_id
        .into_iter()
        .fold(0_u64, |acc, (_, val)| acc + val)
        .saturating_add(total_lock)
}

pub(crate) fn total_gas_in_wl_mb() -> u64 {
    total_gas_in_wait_list() + total_gas_in_mailbox()
}

pub(crate) fn total_reserved_balance() -> u128 {
    // Iterate through all accounts and calculate the cumulative reserved balance
    <system::Account<Runtime>>::iter()
        .map(|(_, v)| v.data.reserved)
        .fold(0, |acc, v| acc.saturating_add(v))
}

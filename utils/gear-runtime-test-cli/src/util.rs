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

use codec::Encode;
use frame_support::traits::{Currency, GenesisBuild, OnFinalize, OnIdle, OnInitialize};
use frame_system as system;
use gear_common::{storage::*, Origin};
use gear_core::message::{StoredDispatch, StoredMessage};
use gear_node_primitives::AccountPublic;
#[cfg(feature = "gear-native")]
use gear_runtime::{
    AuraConfig, Authorship, Event, Gear, GearGas, GearMessenger, Runtime, Signature, System,
};
use pallet_gear::{BlockGasLimitOf, Config, GasAllowanceOf};
use pallet_gear_debug::DebugData;
#[cfg(feature = "authoring-aura")]
use sp_consensus_aura::{sr25519::AuthorityId as AuraId, AURA_ENGINE_ID};
#[cfg(not(feature = "authoring-aura"))]
use sp_consensus_babe::{
    digests::{PreDigest, SecondaryPlainPreDigest},
    AuthorityId as BabeId, BABE_ENGINE_ID,
};
use sp_consensus_slots::Slot;
use sp_core::{sr25519, Pair, Public};
#[cfg(not(feature = "authoring-aura"))]
use sp_finality_grandpa::AuthorityId as GrandpaId;
use sp_runtime::{
    app_crypto::UncheckedFrom,
    traits::IdentifyAccount,
    AccountId32, Digest, DigestItem,
};
use system::pallet_prelude::BlockNumberFor;
#[cfg(all(not(feature = "gear-native"), feature = "vara-native"))]
use vara_runtime::{
    Authorship, Event, Gear, GearGas, GearMessenger, Runtime, SessionConfig, SessionKeys, System,
};

pub(crate) type QueueOf<T> = <<T as pallet_gear::Config>::Messenger as Messenger>::Queue;
pub(crate) type MailboxOf<T> = <<T as pallet_gear::Config>::Messenger as Messenger>::Mailbox;

pub fn get_dispatch_queue() -> Vec<StoredDispatch> {
    QueueOf::<Runtime>::iter()
        .map(|v| v.unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e)))
        .collect()
}

pub fn process_queue(snapshots: &mut Vec<DebugData>, mailbox: &mut Vec<StoredMessage>) {
    while !QueueOf::<Runtime>::is_empty() {
        run_to_block(System::block_number() + 1, None, false);
        // Parse data from events
        for event in System::events() {
            if let Event::GearDebug(pallet_gear_debug::Event::DebugDataSnapshot(snapshot)) =
                &event.event
            {
                snapshots.push(snapshot.clone());
            }

            if let Event::Gear(pallet_gear::Event::UserMessageSent { message, .. }) = &event.event {
                mailbox.push(message.clone());
            }
        }
        System::reset_events();
    }
}

pub(crate) fn initialize(new_blk: BlockNumberFor<Runtime>) {
    log::debug!("📦 Initializing block {}", new_blk);

    // All blocks are to be authored by validator at index 0
    let slot = Slot::from(0);
    #[cfg(feature = "gear-native")]
    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode())],
    };
    #[cfg(all(not(feature = "gear-native"), feature = "vara-native"))]
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
pub(crate) fn on_initialize() {
    System::on_initialize(System::block_number());
    Authorship::on_initialize(System::block_number());
    GearGas::on_initialize(System::block_number());
    GearMessenger::on_initialize(System::block_number());
    Gear::on_initialize(System::block_number());
}

// Run on_finalize hooks (in pallets reverse order, as they appear in AllPalletsWithSystem)
pub(crate) fn on_finalize() {
    Gear::on_finalize(System::block_number());
    GearMessenger::on_finalize(System::block_number());
    GearGas::on_finalize(System::block_number());
    Authorship::on_finalize(System::block_number());
    System::on_finalize(System::block_number());
}

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

// Generate authority keys.
#[cfg(feature = "authoring-aura")]
pub(crate) fn authority_keys_from_seed(s: &str) -> (AccountId32, AuraId) {
    (
        get_account_id_from_seed::<sr25519::Public>(s),
        get_from_seed::<AuraId>(s),
    )
}
#[cfg(not(feature = "authoring-aura"))]
pub fn authority_keys_from_seed(s: &str) -> (AccountId32, BabeId, GrandpaId) {
    (
        get_account_id_from_seed::<sr25519::Public>(s),
        get_from_seed::<BabeId>(s),
        get_from_seed::<GrandpaId>(s),
    )
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::default()
        .build_storage::<Runtime>()
        .unwrap();

    let authorities = vec![authority_keys_from_seed("Val")];
    let balances = vec![
        (
            AccountId32::unchecked_from(1000001.into_origin()),
            (BlockGasLimitOf::<Runtime>::get() * 20) as u128,
        ),
        (
            AccountId32::unchecked_from(crate::HACK.into_origin()),
            (BlockGasLimitOf::<Runtime>::get() * 20) as u128,
        ),
    ];

    pallet_balances::GenesisConfig::<Runtime> {
        balances: balances
            .into_iter()
            .chain(
                authorities
                    .iter()
                    .cloned()
                    .map(|(acc, ..)| (acc, <Runtime as Config>::Currency::minimum_balance())),
            )
            .collect(),
    }
    .assimilate_storage(&mut t)
    .unwrap();

    #[cfg(feature = "authoring-aura")]
    AuraConfig {
        authorities: authorities.iter().cloned().map(|(_, x)| x).collect(),
    }
    .assimilate_storage(&mut t)
    .unwrap();
    #[cfg(not(feature = "authoring-aura"))]
    SessionConfig {
        keys: authorities
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
            .collect(),
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| {
        initialize(1);
        on_initialize();
    });
    ext
}

pub fn run_to_block(n: u32, remaining_weight: Option<u64>, skip_process_queue: bool) {
    while System::block_number() < n {
        on_finalize();
        initialize(System::block_number() + 1);
        on_initialize();
        let remaining_weight = remaining_weight.unwrap_or_else(BlockGasLimitOf::<Runtime>::get);
        if skip_process_queue {
            GasAllowanceOf::<Runtime>::put(remaining_weight);
        } else {
            Gear::on_idle(System::block_number(), remaining_weight);
        }
    }
}

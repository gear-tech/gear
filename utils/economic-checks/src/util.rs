// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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
use frame_support::{
    assert_ok,
    traits::{OffchainWorker, OnFinalize, OnIdle, OnInitialize},
};
use frame_system as system;
use gear_runtime::{Call, Gear, Runtime, System, UncheckedExtrinsic, Usage};
use parking_lot::RwLock;
use primitive_types::H256;
use sp_core::offchain::{
    testing::{PoolState, TestOffchainExt, TestTransactionPoolExt},
    Duration, OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
};
use sp_io::offchain;
use sp_runtime::AccountId32;
use std::sync::Arc;

pub fn new_test_ext(balances: Vec<(impl Into<AccountId32>, u128)>) -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::default()
        .build_storage::<Runtime>()
        .unwrap();

    pallet_balances::GenesisConfig::<Runtime> {
        balances: balances
            .into_iter()
            .map(|(acc, balance)| (acc.into(), balance))
            .collect(),
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

pub fn with_offchain_ext(
    balances: Vec<(impl Into<AccountId32>, u128)>,
) -> (sp_io::TestExternalities, Arc<RwLock<PoolState>>) {
    let mut ext = new_test_ext(balances);
    let (offchain, _) = TestOffchainExt::new();
    let (pool, pool_state) = TestTransactionPoolExt::new();

    ext.register_extension(OffchainDbExt::new(offchain.clone()));
    ext.register_extension(OffchainWorkerExt::new(offchain));
    ext.register_extension(TransactionPoolExt::new(pool));

    (ext, pool_state)
}

pub(crate) fn run_to_block(n: u32, remaining_weight: Option<u64>) {
    while System::block_number() < n {
        System::on_finalize(System::block_number());
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        Gear::on_initialize(System::block_number());
        let remaining_weight =
            remaining_weight.unwrap_or(<Runtime as pallet_gear::Config>::BlockGasLimit::get());
        Gear::on_idle(System::block_number(), remaining_weight);
    }
}

pub(crate) fn run_to_block_with_ocw(
    n: u32,
    pool: Arc<RwLock<PoolState>>,
    remaining_weight: Option<u64>,
) {
    let now = System::block_number();
    for i in now + 1..=n {
        System::on_finalize(i - 1);
        System::set_block_number(i);
        System::on_initialize(i);
        Gear::on_initialize(i);
        let remaining_weight =
            remaining_weight.unwrap_or(<Runtime as pallet_gear::Config>::BlockGasLimit::get());
        Gear::on_idle(i, remaining_weight);
        process_tx_pool(pool.clone());
        increase_offchain_time(1_000);
        Usage::offchain_worker(i);
    }
}

fn increase_offchain_time(ms: u64) {
    offchain::sleep_until(offchain::timestamp().add(Duration::from_millis(ms)));
}

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

pub(crate) fn generate_program_id(code: &[u8], salt: &[u8]) -> H256 {
    // TODO #512
    let mut data = Vec::new();
    code.encode_to(&mut data);
    salt.encode_to(&mut data);

    sp_io::hashing::blake2_256(&data[..]).into()
}

pub(crate) fn process_tx_pool(pool: Arc<RwLock<PoolState>>) {
    let mut guard = pool.write();
    guard.transactions.iter().cloned().for_each(|bytes| {
        let tx = UncheckedExtrinsic::decode(&mut &bytes[..]).unwrap();
        if let Call::Usage(pallet_usage::Call::collect_waitlist_rent { payees_list }) = tx.function
        {
            log::debug!(
                "Sending collect_wait_list extrinsic with payees_list {:?}",
                payees_list
            );
            assert_ok!(Usage::collect_waitlist_rent(
                system::RawOrigin::None.into(),
                payees_list
            ));
        }
    });
    guard.transactions = vec![];
}

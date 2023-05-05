// This file is part of Gear.
//
// Copyright (C) 2023 Gear Technologies Inc.
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

use frame_remote_externalities::{Mode, OnlineConfig, RemoteExternalities, Transport};
use frame_support::traits::OnRuntimeUpgrade;
use gear_runtime::Block;
use migration_tests::MigrationTest;

pub fn run_upgrade<T: MigrationTest + OnRuntimeUpgrade>(ext: &mut RemoteExternalities<Block>) {
    ext.execute_with(|| {
        log::info!("Running `OnRuntimeUpgrade::pre_upgrade()`");
        let substrate_state = T::pre_upgrade().unwrap();
        log::info!("Running `MigrationTest::pre_migration()`");
        let test_state = T::pre_migration();
        log::info!("Running runtime upgrade");
        let weight = T::on_runtime_upgrade();
        log::info!("Running `OnRuntimeUpgrade::post_upgrade()`");
        T::post_upgrade(substrate_state).unwrap();
        log::info!("Running `MigrationTest::pre_migration()`");
        T::post_migration(test_state);
        log::info!("Runtime upgrade weight: {weight}");
    });
}

pub fn new_remote_ext(mode: Mode<Block>) -> RemoteExternalities<Block> {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        log::info!("Building remote externalities");
        frame_remote_externalities::Builder::new()
            .mode(mode)
            .build()
            .await
            .unwrap()
    })
}

pub fn latest_gear_ext() -> RemoteExternalities<Block> {
    new_remote_ext(Mode::Online(OnlineConfig {
        // TODO: change URI to private RPC node
        transport: Transport::Uri("wss://rpc-node.gear-tech.io:443".to_string()),
        ..Default::default()
    }))
}

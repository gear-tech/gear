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

use frame_remote_externalities::{Mode, OfflineConfig, RemoteExternalities, SnapshotConfig};
use gear_runtime::{Block, Runtime};
use migration_tests::{new_remote_ext, run_upgrade};
use pallet_gear_program::migration::MigrateV1ToV2;

fn new_test_ext_v130() -> RemoteExternalities<Block> {
    new_remote_ext(Mode::Offline(OfflineConfig {
        state_snapshot: SnapshotConfig {
            path: "snapshots/gear-staging-testnet-130.snap".into(),
        },
    }))
}

#[test]
fn migration_test() {
    env_logger::init();
    let mut ext = new_test_ext_v130();
    run_upgrade::<MigrateV1ToV2<Runtime>>(&mut ext);
}

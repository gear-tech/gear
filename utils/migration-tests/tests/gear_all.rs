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

use gear_runtime::{AllPalletsWithSystem, Migrations};
use migration_tests::{latest_gear_ext, run_upgrade};

#[ignore]
#[test]
fn migration_test() {
    env_logger::init();
    let mut ext = latest_gear_ext();
    run_upgrade::<(Migrations, AllPalletsWithSystem)>(&mut ext);
}

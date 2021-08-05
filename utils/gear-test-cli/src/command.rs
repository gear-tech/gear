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

use sc_cli::{CliConfiguration, SharedParams};
use sc_service::Configuration;

use frame_system as system;

use crate::GearTestCmd;

pub fn new_test_ext() -> sp_io::TestExternalities {
    system::GenesisConfig::default()
        .build_storage::<gear_runtime::Runtime>()
        .unwrap()
        .into()
}

impl GearTestCmd {
    /// Runs tests from `.yaml` files.
    pub fn run(&self, _config: Configuration) -> sc_cli::Result<()> {
        new_test_ext().execute_with(|| {
            gear_test::check::check_main(self.input.to_vec(), true, false, false, || {
                sp_io::storage::clear_prefix(b"g::code");
                sp_io::storage::clear_prefix(b"g::alloc");
                sp_io::storage::clear_prefix(b"g::msg");
                sp_io::storage::clear_prefix(b"g::prog");
                gear_core::storage::Storage {
                    message_queue: rti::ext::ExtMessageQueue::default(),
                    program_storage: rti::ext::ExtProgramStorage,
                }
            })
            .expect("what is it failed?");
        });

        Ok(())
    }
}

impl CliConfiguration for GearTestCmd {
    fn shared_params(&self) -> &SharedParams {
        &self.shared_params
    }
}

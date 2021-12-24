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

use crate::manager::RuntestsExtManager;
use crate::GearTestCmd;
use gear_backend_sandbox::SandboxEnvironment;
use gear_core_processor::Ext;
use gear_runtime::Runtime;
use sc_cli::{CliConfiguration, SharedParams};
use sc_service::Configuration;

pub fn new_test_ext() -> sp_io::TestExternalities {
    frame_system::GenesisConfig::default()
        .build_storage::<Runtime>()
        .unwrap()
        .into()
}

impl GearTestCmd {
    /// Runs tests from `.yaml` files.
    pub fn run(&self, _config: Configuration) -> sc_cli::Result<()> {
        new_test_ext()
            .execute_with(|| {
                gear_test::check::check_main::<
                    RuntestsExtManager<Runtime>,
                    SandboxEnvironment<Ext>,
                    _,
                >(
                    self.input.to_vec(),
                    false,
                    false,
                    false,
                    false,
                    || {
                        sp_io::storage::clear_prefix(gear_common::STORAGE_CODE_PREFIX, None);
                        sp_io::storage::clear_prefix(gear_common::STORAGE_MESSAGE_PREFIX, None);
                        sp_io::storage::clear_prefix(gear_common::STORAGE_PROGRAM_PREFIX, None);
                        sp_io::storage::clear_prefix(gear_common::STORAGE_WAITLIST_PREFIX, None);
                        Default::default()
                    },
                    Some(Box::new(&new_test_ext)),
                )
            })
            .map_err(|e: anyhow::Error| sc_cli::Error::Application(e.into()))
    }
}

impl CliConfiguration for GearTestCmd {
    fn shared_params(&self) -> &SharedParams {
        &self.shared_params
    }
}

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

use codec::Decode;
use rti::ext::{ExtAllocationStorage, ExtProgramStorage};
use rti::runner::ExtRunner;
use test_gear_sample::sample::Test;

use gear_core::{message::Message, storage::Storage};

use frame_system as system;

pub fn new_test_ext() -> sp_io::TestExternalities {
    system::GenesisConfig::default()
        .build_storage::<gear_runtime::Runtime>()
        .unwrap()
        .into()
}

pub fn init_fixture(
    ext: &mut sp_io::TestExternalities,
    test: &Test,
    fixture_no: usize,
) -> anyhow::Result<ExtRunner> {
    ext.execute_with(|| {
        // Dispatch a signed extrinsic.

        let mut runner = rti::runner::new();
        for program in test.programs.iter() {
            let code = std::fs::read(program.path.clone())?;
            let mut init_message = Vec::new();
            if let Some(init_msg) = &program.init_message {
                init_message = init_msg.clone().into_raw();
            }
            runner.init_program(program.id.into(), code, init_message, u64::max_value())?;
        }
        let fixture = &test.fixtures[fixture_no];
        for message in fixture.messages.iter() {
            runner.queue_message(
                message.destination.into(),
                message.payload.clone().into_raw(),
                Some(u64::max_value()),
            )
        }

        Ok(runner)
    })
}

pub struct FinalState {
    pub message_queue: Vec<Message>,
    pub allocation_storage: ExtAllocationStorage,
    pub program_storage: ExtProgramStorage,
}

pub fn run(
    ext: &mut sp_io::TestExternalities,
    mut runner: ExtRunner,
    steps: Option<u64>,
) -> anyhow::Result<(FinalState, Vec<u8>)> {
    ext.execute_with(|| {
        if let Some(steps) = steps {
            for _ in 0..steps {
                runner.run_next()?;
            }
        } else {
            while runner.run_next()? > 0 {}
        }
        let message_queue = sp_io::storage::get(b"g::msg")
            .map(|val| Vec::<Message>::decode(&mut &val[..]).expect("values encoded correctly"))
            .unwrap_or_default();

        let (
            Storage {
                message_queue: _,
                allocation_storage,
                program_storage,
            },
            persistent_memory,
        ) = runner.complete();

        Ok((
            FinalState {
                message_queue,
                allocation_storage,
                program_storage,
            },
            persistent_memory,
        ))
    })
}

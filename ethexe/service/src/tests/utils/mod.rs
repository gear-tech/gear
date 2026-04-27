// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

pub use env::*;
use ethexe_db::{GenesisInitializer, dump::StateDump};
use ethexe_processor::Processor;
pub use events::*;
use futures::FutureExt;

mod env;
mod events;

use tracing_subscriber::EnvFilter;

pub fn init_logger() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .try_init();
}

pub struct GenesisInitializerFromDump {
    pub dump: Option<StateDump>,
    pub processor: Processor,
}

impl GenesisInitializer for GenesisInitializerFromDump {
    fn get_genesis_data(&mut self) -> anyhow::Result<StateDump> {
        self.dump
            .take()
            .ok_or_else(|| anyhow::anyhow!("genesis data already consumed"))
    }

    fn process_code(
        &mut self,
        code_id: gprimitives::CodeId,
        code: Vec<u8>,
    ) -> ethexe_db::CodeProcessingFuture {
        let mut cloned_processor = self.processor.clone();
        async move {
            let info = cloned_processor
                .process_code(ethexe_common::CodeAndIdUnchecked { code_id, code })
                .await?;

            let Some(valid) = info.valid else {
                return Ok(None);
            };
            Ok(Some((valid.instrumented_code, valid.code_metadata)))
        }
        .boxed()
    }
}

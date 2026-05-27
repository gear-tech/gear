// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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

/// Helper for visually separating important info messages in test logs. Example:
/// ```text
///    12.345s  INFO
///    -----------------------------------
///           centralized info message
///    -----------------------------------
/// ```
#[allow(unused_macros)]
macro_rules! test_info {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        let bar_width = (msg.len() + 8).max(40);
        let bar = "-".repeat(bar_width);
        let lpad = " ".repeat(bar_width.saturating_sub(msg.len()) / 2);
        log::info!("\n{bar}\n{lpad}{msg}\n{bar}");
    }};
}

#[allow(unused_imports)]
pub(crate) use test_info;

#[allow(dead_code)]
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

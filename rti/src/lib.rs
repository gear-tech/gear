#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
pub mod ext;

#[cfg(feature = "std")]
pub mod runner;

use sp_runtime_interface::runtime_interface;
use codec::{Encode, Decode};
use sp_core::H256;

#[cfg(not(feature = "std"))]
use sp_std::prelude::Vec;
#[cfg(feature = "std")]
use gear_core::storage::Storage;

#[derive(Debug, Encode, Decode)]
pub enum Error {
    Trap,
    Runner,
}

#[derive(Debug, Encode, Decode)]
pub struct ExecutionReport {
    pub handled: u32,
    pub log: Vec<(H256, Vec<u8>)>,
}

#[runtime_interface]
pub trait GearExecutor {
    fn process() -> Result<ExecutionReport, Error> {
        let mut runner = crate::runner::new();
        let handled = runner.run_next().map_err(|e| {
            log::error!("Error handling message: {:?}", e);
            Error::Runner
        })?;

        let (Storage { message_queue, .. }, persistent_memory) = runner.complete();
        let log = message_queue.log.into_iter().map(
            |msg| (
                H256::from_slice(msg.source.as_slice()),
                msg.payload.into_raw()
            )
        ).collect::<Vec<_>>();

        crate::runner::set_memory(persistent_memory);

        Ok(
            ExecutionReport { 
                handled: handled as _,
                log,
            }
        )
    }
}

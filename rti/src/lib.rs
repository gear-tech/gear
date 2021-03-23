#[cfg(feature = "std")]
mod ext;

#[cfg(feature = "std")]
mod runner;

use sp_runtime_interface::runtime_interface;
use codec::{Encode, Decode};

#[derive(Debug, Encode, Decode)]
pub enum Error {
    Trap,
    Runner,
}

#[derive(Debug, Encode, Decode)]
pub struct ExecutionReport {
    pub handled: u32,
}

#[runtime_interface]
pub trait GearExecutor {
    fn process(&mut self) -> Result<ExecutionReport, Error> {
        let mut runner = crate::runner::new();
        let handled = runner.run_next().map_err(|e| {
            log::warn!("Error handling message: {:?}", e);
            Error::Runner
        })?;

        let (_, persistent_memory) = runner.complete();

        crate::runner::set_memory(persistent_memory);

        Ok(ExecutionReport { handled: handled as _ })
    }
}

#[cfg(feature = "std")]
mod ext;

use sp_runtime_interface::runtime_interface;
use codec::{Encode, Decode};

#[derive(Debug, Encode, Decode)]
pub enum Error {
    Trap,
}

#[derive(Debug, Encode, Decode)]
pub struct ExecutionReport {
    pub handled: u32,
}

#[runtime_interface]
pub trait GearExecutor {
    fn process(&mut self) -> Result<ExecutionReport, Error> {
        unimplemented!()
    }
}

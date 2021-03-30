use crate::GearTestCmd;
use codec::{Decode, Encode};
use sc_cli::{CliConfiguration, ExecutionStrategy, Result, SharedParams};
use sc_service::{Configuration, NativeExecutionDispatch};
use std::fmt::Debug;
use frame_system as system;

pub fn new_test_ext() -> sp_io::TestExternalities {
    system::GenesisConfig::default().build_storage::<gear_runtime::Runtime>().unwrap().into()
}

impl GearTestCmd {
    /// Runs the command and benchmarks the chain.
    pub fn run(&self, config: Configuration) -> Result<()> {
        println!("{:?}", self.input);
        new_test_ext().execute_with(|| {
            // Dispatch a signed extrinsic.

            let mut runner = rti::runner::new();
            runner.queue_message(1.into(), Vec::new().into());
            runner.run_next();
        });

        Ok(())
    }
}

impl CliConfiguration for GearTestCmd {
	fn shared_params(&self) -> &SharedParams {
		&self.shared_params
	}
}
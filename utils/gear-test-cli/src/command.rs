
use crate::GearTestCmd;
use crate::mock::*;
use sc_cli::{SharedParams, CliConfiguration, ExecutionStrategy, Result};
use codec::{Decode, Encode};
use sc_service::{Configuration, NativeExecutionDispatch};
use std::fmt::Debug;
use common::*;
use frame_support::assert_ok;

impl GearTestCmd {
	/// Runs the command and benchmarks the chain.
	pub fn run(&self, config: Configuration) -> Result<()>
	{
		new_test_ext().execute_with(|| {
			// Dispatch a signed extrinsic.

				GearModule::submit_program(
					Origin::signed(1),
					Program { static_pages: Vec::new(), code: Vec::new() }
				)
		});
		Ok(())


	}
}

impl CliConfiguration for GearTestCmd {
	fn shared_params(&self) -> &SharedParams {
		&self.shared_params
	}
}
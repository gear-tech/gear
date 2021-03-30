use std::fmt::Debug;

mod command;
// mod mock;

/// The `benchmark` command used to benchmark FRAME Pallets.
#[derive(Debug, structopt::StructOpt)]
pub struct GearTestCmd {
	/// Input json file for testing.
	pub input: Option<std::path::PathBuf>,
	
	#[allow(missing_docs)]
	#[structopt(flatten)]
	pub shared_params: sc_cli::SharedParams,
}

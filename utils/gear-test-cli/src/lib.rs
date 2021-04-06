use std::fmt::Debug;

mod command;
mod test_runner;

/// The `runtests` command used to test gear with json.
#[derive(Debug, structopt::StructOpt)]
pub struct GearTestCmd {
    /// Input dir/file with json for testing.
    #[structopt(parse(from_os_str))]
    pub input: Vec<std::path::PathBuf>,

    #[allow(missing_docs)]
    #[structopt(flatten)]
    pub shared_params: sc_cli::SharedParams,
}

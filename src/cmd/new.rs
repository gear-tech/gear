//! command new
use structopt::StructOpt;

/// Create a new gear program
#[derive(Debug, StructOpt)]
pub struct New {
    /// create gear program from template
    #[structopt(short, long)]
    pub template: Option<String>,
}

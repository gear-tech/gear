//! command update
use structopt::StructOpt;

/// Update resources
#[derive(Debug, StructOpt)]
pub struct Update {
    /// update all resources
    #[structopt(short, long)]
    pub all: bool,
    /// update template list
    #[structopt(short, long)]
    pub template: bool,
    /// update self
    #[structopt(short, long)]
    pub gear: bool,
}

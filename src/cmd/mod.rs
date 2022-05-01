//! commands
use structopt::StructOpt;

// mod deploy;
// mod login;
mod new;
mod update;

#[derive(Debug, StructOpt)]
pub enum Command {
    New(new::New),
    Update(update::Update),
}

#[derive(Debug, StructOpt)]
#[structopt(name = "gear-program")]
pub struct Opt {
    /// Enable debug logs
    #[structopt(short, long)]
    pub debug: bool,
    #[structopt(subcommand)]
    pub command: Command,
}

impl Opt {
    /// run program
    pub fn run() {
        Opt::from_args();
    }
}

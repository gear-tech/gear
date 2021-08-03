mod check;
mod runner;
mod sample;

use gear_core::storage;

use clap::{AppSettings, Clap};

#[derive(Clap)]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    /// Skip messages checks
    #[clap(long)]
    pub skip_messages: bool,
    /// Skip allocations checks
    #[clap(long)]
    pub skip_allocations: bool,
    /// Skip memory checks
    #[clap(long)]
    pub skip_memory: bool,
    /// JSON sample file(s) or dir
    pub input: Vec<std::path::PathBuf>,
    /// A level of verbosity, and can be used multiple times
    #[clap(short, long, parse(from_occurrences))]
    verbose: i32,
}

pub fn main() -> anyhow::Result<()> {
    let opts: Opts = Opts::parse();
    let mut print_log = false;
    match opts.verbose {
        0 => env_logger::init(),
        1 => {
            print_log = true;
        }
        2 => {
            use env_logger::Env;

            print_log = true;
            env_logger::Builder::from_env(
                Env::default().default_filter_or("gear_core_backend=debug"),
            )
            .init();
        }
        _ => {
            use env_logger::Env;

            env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
        }
    }

    check::check_main(
        opts.input.to_vec(),
        opts.skip_messages,
        opts.skip_allocations,
        opts.skip_memory,
        print_log,
        || storage::new_in_memory_empty(),
    )
}

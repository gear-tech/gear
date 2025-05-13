use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lazy page fuzzer", version, about = "lazy pages fuzzer")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run fuzzer normally
    Run(RunArgs),

    /// Reproduce fuzzer run with a specific instance seed
    Reproduce {
        /// 64-char hex string representing [u8; 32]
        instance_seed: String,
    },
}

#[derive(Args)]
pub struct RunArgs {
    /// Don't run the fuzzer, just print the module and exit
    #[arg(long, default_value_t = false)]
    pub print_module_and_exit: bool,
}

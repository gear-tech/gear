use clap::Parser;
use gear_validator_checks::Opt;

#[tokio::main]
async fn main() {
    if let Err(e) = Opt::parse().run().await {
        log::error!("{}", e);
        std::process::exit(1);
    }
}

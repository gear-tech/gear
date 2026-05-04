use clap::Parser;
use gear_validator_checks::Opt;

#[tokio::main]
async fn main() {
    Opt::parse().run().await.unwrap()
}

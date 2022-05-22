//! gear command entry

#[tokio::main]
async fn main() {
    if let Err(e) = gear_program::Opt::run().await {
        println!("{}", e);
    }
}

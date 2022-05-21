//! gear command entry

#[tokio::main]
async fn main() {
    gear_program::Opt::run().await.unwrap();
}

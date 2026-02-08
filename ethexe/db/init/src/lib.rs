#[cfg(feature = "mock")]
use ethexe_db::{Database, DatabaseRef, MemDb};
use gsigner::Address;

pub use version1::*;

mod version1;

pub const DB_VERSION_0: u32 = 0;
pub const DB_VERSION_1: u32 = 1;

pub struct InitConfig {
    pub ethereum_rpc: String,
    pub router_address: Address,
    pub slot_duration_secs: u64,
}

#[cfg(feature = "mock")]
pub async fn create_initialized_empty_memory_db(config: InitConfig) -> anyhow::Result<Database> {
    let db = MemDb::default();
    initialize_empty_db(config, DatabaseRef { kv: &db, cas: &db }).await?;
    Database::from_one(&db)
}
